// JNI bridge for Hey's BEAM shim. Class: os.elastos.hey.social.BeamApi.
// Phase A: address + validate_token are LOCAL (no node) and implemented here. balance/send/scan
// need the FlyClient reactor + a node (Phase B) and remain safe stubs.
// BEAM wallet-core calls are marked // VERIFY: confirm the exact signature against the
// beam-7.5.13882 headers on the first compile (see docs/HEY_BEAM_INTEGRATION.md).
#include <jni.h>
#include <string>
#include <vector>
#include <cstdint>
#include <atomic>
#include <mutex>
#include <future>
#include <netdb.h>
#include <sys/socket.h>
#include <unistd.h>
#include <fcntl.h>
#include "hey_beam.h"
#include <android/log.h>

// BEAM wallet-core (paths relative to BEAM_SRC, added as an include dir by jni/CMakeLists.txt)
#include "wallet/core/wallet_db.h"
#include "wallet/core/common.h"
#include "wallet/core/secstring.h"
#include "wallet/core/wallet.h"
#include "wallet/core/wallet_network.h"
#include "wallet/transactions/lelantus/lelantus_reg_creators.h"  // lelantus::RegisterCreators (public_offline push tx)
#include "wallet/transactions/assets/assets_reg_creators.h"      // RegisterAllAssetCreators (BEAMX = confidential asset)
#include "mnemonic/mnemonic.h"
#include "utility/io/reactor.h"
#include "utility/io/timer.h"
#include "utility/logger.h"   // beam::Logger — route BEAM node logs to logcat (diagnosis)
#include "core/block_rw.h"
// On-device mainnet node (Phase B+): run a private beam::Node in-process; the FlyClient wallet
// then talks to it over 127.0.0.1 (loopback). The node serves OUR coins (owner KDF) and syncs
// the chain directly from mainnet peers — no third-party public node sees our wallet activity.
#include "node/node_client.h"            // beam::NodeClient, beam::INodeClientObserver
#include "node/node.h"                    // beam::Node, Node::s_PortDefault
#include "wallet/core/default_peers.h"    // beam::getDefaultPeers() (current mainnet seeds)
#include <memory>

using namespace beam;
using namespace beam::wallet;

// ── small helpers ─────────────────────────────────────────────────────────────
static std::string esc(const std::string& v) {
    std::string o; o.reserve(v.size() + 2);
    for (char c : v) { if (c == '"' || c == '\\') o.push_back('\\'); o.push_back(c); }
    return o;
}
static std::string err(const std::string& m) { return std::string("{\"error\":\"") + esc(m) + "\"}"; }

// master seed = SHA256(BIP39-decoded entropy) — matches BEAM CLI ReadWalletSeed.
static bool seed_from_mnemonic(const std::string& mnemonic, ECC::NoLeak<ECC::uintBig>& out) {
    std::vector<std::string> words; std::string w;
    for (char c : mnemonic) { if (c == ' ') { if (!w.empty()) { words.push_back(w); w.clear(); } } else w.push_back(c); }
    if (!w.empty()) words.push_back(w);
    try {
        auto buf = decodeMnemonic(words);                 // ByteBuffer decodeMnemonic(const vector<string>&)
        beam::SecString seed;
        seed.assign(buf.data(), buf.size());
        out.V = seed.hash().V;                            // SHA256 via ECC::Hash::Processor
        return true;
    } catch (...) { return false; }
}

// Deterministic DB password from the seed so the persistent DB re-opens across calls.
static beam::SecString db_pass(const ECC::NoLeak<ECC::uintBig>& seed) {
    ECC::Hash::Value h;
    ECC::Hash::Processor() << seed.V >> h;   // deterministic password derived from the seed
    static const char* hexd = "0123456789abcdef";
    std::string s; s.reserve(h.nBytes * 2);
    for (uint32_t i = 0; i < h.nBytes; i++) { s.push_back(hexd[h.m_pData[i] >> 4]); s.push_back(hexd[h.m_pData[i] & 0xf]); }
    return beam::SecString(s);
}

// Open (or create from seed) the persistent WalletDB at dir/beam-wallet.db.
static IWalletDB::Ptr open_db(const std::string& mnemonic, const std::string& dir) {
    ECC::NoLeak<ECC::uintBig> seed;
    if (!seed_from_mnemonic(mnemonic, seed)) return nullptr;
    std::string path = dir + "/beam-wallet.db";
    beam::SecString pass = db_pass(seed);
    try {
        if (WalletDB::isInitialized(path)) return WalletDB::open(path, pass);  // VERIFY: isInitialized(path)
        return WalletDB::init(path, pass, seed);
    } catch (...) { return nullptr; }
}

// AmountBig::Type (128-bit) -> groth string (low 64 bits; BEAM balances fit well under 2^64).
static std::string amount_str(const beam::AmountBig::Type& a) {
    return std::to_string(beam::AmountBig::get_Lo(a));   // VERIFY: get_Lo returns the low Amount (uint64 groth)
}

// ── wallet sync state (polled by Kotlin for the syncing/synced status) ────────────────────────
// Quick-sync only: the wallet does FlyClient light verification against an official public BEAM
// node — no on-device node, no 127.0.0.1 loopback. done/total stay 0 (there's no local block
// sync to report); the UI shows an indeterminate "Syncing…" until `synced` flips.
/// Resolve + TCP-probe "host:port" under a HARD deadline on a worker thread.
/// Returns the NUMERIC "ip:port" (so the caller never re-blocks on DNS), or ""
/// when unreachable. This runs BEFORE the wallet/reactor exist — without it a
/// hung DNS lookup or a filtered port (mobile networks often block :8100)
/// freezes the scan before the 45s reactor watchdog is even armed, and the
/// in-flight guard then blocks every retry until the app restarts.
static std::string probe_node(const std::string& uri, int timeout_ms) {
    auto fut = std::async(std::launch::async, [uri]() -> std::string {
        auto colon = uri.rfind(':');
        if (colon == std::string::npos) return "";
        std::string host = uri.substr(0, colon), port = uri.substr(colon + 1);
        addrinfo hints{}; hints.ai_family = AF_INET; hints.ai_socktype = SOCK_STREAM;
        addrinfo* res = nullptr;
        if (getaddrinfo(host.c_str(), port.c_str(), &hints, &res) != 0 || !res) return "";
        std::string out;
        for (addrinfo* a = res; a; a = a->ai_next) {
            int fd = socket(a->ai_family, SOCK_STREAM, 0);
            if (fd < 0) continue;
            fcntl(fd, F_SETFL, fcntl(fd, F_GETFL, 0) | O_NONBLOCK);
            connect(fd, a->ai_addr, a->ai_addrlen);
            fd_set w; FD_ZERO(&w); FD_SET(fd, &w);
            timeval tv{4, 0};
            if (select(fd + 1, nullptr, &w, nullptr, &tv) == 1) {
                int err = 0; socklen_t l = sizeof(err);
                getsockopt(fd, SOL_SOCKET, SO_ERROR, &err, &l);
                if (err == 0) {
                    char ip[64];
                    if (getnameinfo(a->ai_addr, a->ai_addrlen, ip, sizeof(ip), nullptr, 0, NI_NUMERICHOST) == 0) {
                        out = std::string(ip) + ":" + port;
                    }
                }
            }
            close(fd);
            if (!out.empty()) break;
        }
        freeaddrinfo(res);
        return out;
    });
    if (fut.wait_for(std::chrono::milliseconds(timeout_ms)) != std::future_status::ready) return "";
    return fut.get();
}

/// NON-FATAL multi-seed reachability check (B3 fix). Probes EVERY current mainnet seed (from BEAM's
/// own getDefaultPeers() — never a hardcoded stale single host) with a per-seed budget and an overall
/// cap, returns true as soon as ANY seed answers on TCP. Each seed is probed in its OWN async task so
/// a single slow/cold-DNS host can't eat the whole budget; we wait up to total_cap_ms for the first
/// success. This NEVER aborts node start — it only sets a UI hint. Caller passes BEAM's seed list.
static bool any_seed_reachable(const std::vector<std::string>& seeds, int per_seed_ms, int total_cap_ms) {
    if (seeds.empty()) return false;
    // Each seed reuses probe_node (IPv4, hard per-seed deadline). Launch them all, then collect with
    // an overall deadline so the worst-case wait is ~total_cap_ms, not sum(per_seed_ms).
    std::vector<std::future<std::string>> futs;
    futs.reserve(seeds.size());
    for (const auto& s : seeds)
        futs.emplace_back(std::async(std::launch::async, [s, per_seed_ms]() { return probe_node(s, per_seed_ms); }));
    auto deadline = std::chrono::steady_clock::now() + std::chrono::milliseconds(total_cap_ms);
    bool reachable = false;
    for (auto& f : futs) {
        auto now = std::chrono::steady_clock::now();
        if (now >= deadline) {
            if (f.wait_for(std::chrono::milliseconds(0)) == std::future_status::ready && !f.get().empty()) { reachable = true; }
            continue;   // past the cap — only harvest already-finished probes, don't keep blocking
        }
        if (f.wait_for(deadline - now) == std::future_status::ready) {
            if (!f.get().empty()) { reachable = true; break; }   // first answer wins
        }
        // else: this probe ran out the overall cap; leave it (its detached task self-completes).
    }
    return reachable;
}

static std::atomic<uint64_t> g_sync_done{0};
static std::atomic<uint64_t> g_sync_total{0};
static std::atomic<bool> g_sync_active{false};
static std::atomic<bool> g_synced{false};       // true once the WALLET finished a scan
static std::atomic<uint64_t> g_tip{0};          // last KNOWN chain height from the wallet db (so the
                                                // UI can show "Synced - block N" and users can verify)

// ── H-1 / H1-1: in-process ARM gate for the BEAM signer ─────────────────────────
// `send()` (the standalone C++ BEAM signer) is reachable via the JNI-registered
// BeamApi.beam_send symbol from ANY in-process caller — with NO guard/cap. To bring
// it under the constitution, the ONLY sanctioned caller (the Rust hey_beam_send,
// which first redeems the spend grant + enforces the cap in guard.rs) must ARM this
// flag IMMEDIATELY before invoking beam_send, bound to the EXACT (token, amount_groth,
// asset_id, nonce) it is about to send. `send()` then REFUSES unless armed for exactly
// this transfer AND nonce, and CONSUMES the arm single-use KEYED ON THE NONCE.
//
// H1-1: a FRESH 16-byte random nonce (minted in Rust per send, after redeem_spend +
// check_beam_cap) is the single-use key. Matching on the nonce — not just the
// (token, groth, asset) tuple — means two LEGITIMATE identical transfers (same
// recipient, amount and asset) can never share an arm, and a replayed bare beam_send
// can't race a still-set arm for the same tuple. The nonce binds arm↔send 1:1.
static std::atomic<bool>     g_send_armed{false};
static std::atomic<uint64_t> g_armed_groth{0};
static std::atomic<uint32_t> g_armed_asset{0};
static std::string           g_armed_token;            // guarded by g_arm_mtx
static std::string           g_armed_nonce;            // guarded by g_arm_mtx (the single-use key)
static std::mutex            g_arm_mtx;

// Arm the next beam_send for exactly (token, amount_groth, asset_id, nonce). Single shot:
// any prior arm is overwritten (a fresh redeem always precedes a fresh arm in Rust).
static void arm_send(const std::string& token, uint64_t amount_groth, uint32_t asset_id,
                     const std::string& nonce) {
    std::lock_guard<std::mutex> lk(g_arm_mtx);
    g_armed_token = token;
    g_armed_groth = amount_groth;
    g_armed_asset = asset_id;
    g_armed_nonce = nonce;
    g_send_armed  = true;
}
// Consume the arm IFF it matches exactly, KEYED ON THE NONCE. Returns true (and clears the
// arm) on a match; false (and clears the arm) on a mismatch — single-use either way, so a
// failed attempt can't be retried against the same arm. A blank nonce never matches
// (defends against an unarmed bare call that happens to pass the tuple).
static bool consume_arm(const std::string& token, uint64_t amount_groth, uint32_t asset_id,
                        const std::string& nonce) {
    std::lock_guard<std::mutex> lk(g_arm_mtx);
    bool ok = g_send_armed.load()
        && !nonce.empty()
        && g_armed_nonce == nonce
        && g_armed_groth.load() == amount_groth
        && g_armed_asset.load() == asset_id
        && g_armed_token == token;
    // Single-use: clear the arm whether or not it matched.
    g_send_armed = false;
    g_armed_token.clear();
    g_armed_nonce.clear();
    g_armed_groth = 0;
    g_armed_asset = 0;
    return ok;
}

namespace heybeam {

std::string address(const std::string& mnemonic, const std::string& dir) {
    if (mnemonic.empty() || dir.empty()) return err("beam: missing args");
    // BEAM's WalletDB schedules an internal flush timer on the CURRENT io::Reactor; without one,
    // createAddress() dereferences a null reactor and SIGSEGVs. A LOCAL reactor (no node) is enough.
    beam::io::Reactor::Ptr _reactor = beam::io::Reactor::create();
    beam::io::Reactor::Scope _rscope(*_reactor);
    auto db = open_db(mnemonic, dir);
    if (!db) return err("beam: wallet db open failed");
    try {
        WalletAddress addr;
        db->createAddress(addr);                                              // VERIFY: createAddress(WalletAddress&)
        addr.setExpirationStatus(WalletAddress::ExpirationStatus::Never);     // VERIFY: enum name
        addr.m_label = "Hey";
        db->saveAddress(addr);
        std::string token = GenerateToken(TokenType::Public, addr, db);       // VERIFY: GenerateToken(TokenType, WalletAddress, IWalletDB::Ptr)
        if (token.empty()) return err("beam: token generation failed");
        return std::string("{\"token\":\"") + esc(token) + "\"}";
    } catch (const std::exception& e) { return err(std::string("beam: ") + e.what()); }
      catch (...) { return err("beam: address failed"); }
}

std::string validate_token(const std::string& token) {
    if (token.empty()) return "{\"valid\":false}";
    try {
        auto p = ParseParameters(token);                                     // VERIFY: optional<TxParameters> ParseParameters(const string&)
        if (!p) return "{\"valid\":false}";
        return "{\"valid\":true,\"type\":\"public_offline\"}";               // VERIFY: read the actual address type from p
    } catch (...) { return "{\"valid\":false}"; }
}

// ── Phase B (node + FlyClient reactor) ────────────────────────────────────────

// Read the (last-synced) totals from the WalletDB: BEAM (asset 0) + BEAMX (asset 7). No network —
// instant; reflects whatever the last scan() pulled in. groth strings.
std::string balance(const std::string& mnemonic, const std::string& dir, const std::string&) {
    if (mnemonic.empty() || dir.empty()) return err("beam: missing args");
    beam::io::Reactor::Ptr _reactor = beam::io::Reactor::create();   // WalletDB needs a current reactor
    beam::io::Reactor::Scope _rscope(*_reactor);
    auto db = open_db(mnemonic, dir);
    if (!db) return err("beam: wallet db open failed");
    try {
        beam::wallet::storage::Totals totals(*db, false);            // VERIFY: storage::Totals(IWalletDB&, bool)
        const auto& b = totals.GetBeamTotals();
        beam::AmountBig::Type bAvail = b.Avail; bAvail += b.AvailShielded;
        beam::AmountBig::Type bMat   = b.Maturing; bMat += b.MaturingShielded;
        auto x = totals.GetTotals(7);                                // BEAMX = asset id 7  // VERIFY: GetTotals(Asset::ID)
        beam::AmountBig::Type xAvail = x.Avail; xAvail += x.AvailShielded;
        return std::string("{\"beam\":{\"available\":\"") + amount_str(bAvail) +
               "\",\"maturing\":\"" + amount_str(bMat) + "\"},\"beamx\":{\"available\":\"" +
               amount_str(xAvail) + "\"}}";
    } catch (const std::exception& e) { return err(std::string("beam: ") + e.what()); }
      catch (...) { return err("beam: balance failed"); }
}
// ── send helpers ──────────────────────────────────────────────────────────────
static std::string bytes_to_hex(const uint8_t* p, size_t n) {
    static const char* h = "0123456789abcdef";
    std::string s; s.reserve(n * 2);
    for (size_t i = 0; i < n; i++) { s.push_back(h[p[i] >> 4]); s.push_back(h[p[i] & 0xf]); }
    return s;
}
static bool hex_to_bytes(const std::string& s, uint8_t* out, size_t n) {
    if (s.size() != n * 2) return false;
    auto v = [](char c) -> int { if (c >= '0' && c <= '9') return c - '0'; c |= 0x20; if (c >= 'a' && c <= 'f') return c - 'a' + 10; return -1; };
    for (size_t i = 0; i < n; i++) { int hi = v(s[2*i]), lo = v(s[2*i+1]); if (hi < 0 || lo < 0) return false; out[i] = (uint8_t)((hi << 4) | lo); }
    return true;
}
static std::string tx_status_str(beam::wallet::TxStatus st) {
    using S = beam::wallet::TxStatus;                                          // VERIFY: enum members
    switch (st) {
        case S::Completed: return "confirmed";
        case S::Failed:    return "failed";
        case S::Canceled:  return "canceled";
        default:           return "pending";   // Pending / InProgress / Registering / Confirming
    }
}

// Build + broadcast a payment to a recipient token. asset_id 0 = BEAM, 7 = BEAMX (same wallet; the
// asset is a tx-level field). The recipient token's TYPE (a public_offline token => Lelantus push)
// rides in ParseParameters, so the right transaction creator must be registered.
std::string send(const std::string& mnemonic, const std::string& dir, const std::string& node_uri,
                 const std::string& token, uint64_t amount_groth, uint64_t fee_groth, uint32_t asset_id,
                 const std::string& nonce) {
    if (mnemonic.empty() || dir.empty() || node_uri.empty() || token.empty()) return err("beam: missing args");
    if (amount_groth == 0) return err("beam: amount is zero");
    // H-1/H1-1: the signer is under the constitution. It runs ONLY when the Rust path
    // (which already redeemed the spend grant + enforced the cap) armed this exact
    // (token, amount_groth, asset_id, nonce). Consume single-use KEYED ON THE NONCE,
    // fail-closed otherwise — a bare in-process call to BeamApi.beam_send with no arm
    // (or a stale/replayed nonce) is refused here.
    if (!consume_arm(token, amount_groth, asset_id, nonce))
        return err("beam: transfer not authorized (no matching authorization)");
    try {
        beam::Rules::get().UpdateChecksum();                                  // needed to validate the chain
        beam::io::Reactor::Ptr reactor = beam::io::Reactor::create();
        beam::io::Reactor::Scope scope(*reactor);
        auto db = open_db(mnemonic, dir);
        if (!db) return err("beam: wallet db open failed");

        auto params = ParseParameters(token);                                  // VERIFY: optional<TxParameters> ParseParameters(const string&)
        if (!params) return err("beam: invalid recipient token");
        params->SetParameter(TxParameterID::Amount, (beam::Amount) amount_groth);   // VERIFY: TxParameters::SetParameter
        params->SetParameter(TxParameterID::Fee, (beam::Amount) fee_groth);
        if (asset_id != 0)
            params->SetParameter(TxParameterID::AssetID, (beam::Asset::ID) asset_id);  // BEAMX = 7  // VERIFY: AssetID param + Asset::ID

        auto wallet = std::make_shared<beam::wallet::Wallet>(
            db, beam::wallet::Wallet::TxCompletedAction(), beam::wallet::Wallet::UpdateCompletedAction());
        // The Wallet ctor auto-registers the Simple tx creator (wallet.cpp). Add Lelantus (a
        // public_offline token resolves to a Lelantus push tx) + the confidential-asset creators
        // (BEAMX) so StartTransaction recognizes those types. Mirrors wallet/cli/cli.cpp.
        beam::wallet::lelantus::RegisterCreators(*wallet, db);
        beam::wallet::RegisterAllAssetCreators(*wallet);

        beam::io::Address nodeAddr;
        if (!nodeAddr.resolve(node_uri.c_str())) return err("beam: bad node address");
        auto nnet = std::make_shared<beam::proto::FlyClient::NetworkStd>(*wallet);
        nnet->m_Cfg.m_vNodes.push_back(nodeAddr);
        nnet->Connect();
        auto wnet = std::make_shared<beam::wallet::WalletNetworkViaBbs>(*wallet, nnet, db);
        wallet->AddMessageEndpoint(wnet);
        wallet->SetNodeEndpoint(nnet);

        beam::wallet::TxID txid = wallet->StartTransaction(*params);            // VERIFY: TxID StartTransaction(const TxParameters&)
        std::string txid_hex = bytes_to_hex(txid.data(), txid.size());

        // The reactor must RUN for the tx to build + broadcast. The tx is PERSISTED, so even if we
        // return at the timeout the always-on wallet keeps broadcasting and the UI polls tx_status.
        wallet->ResumeAllTransactions();                                       // VERIFY
        beam::io::Timer::Ptr timer = beam::io::Timer::create(*reactor);
        timer->start(60000, false, []() { beam::io::Reactor::get_Current().stop(); });
        reactor->run();

        std::string status = "pending";
        try { auto tx = db->getTx(txid); if (tx) status = tx_status_str(tx->m_status); } catch (...) {}  // VERIFY: getTx(const TxID&), TxDescription::m_status
        return std::string("{\"txid\":\"") + esc(txid_hex) + "\",\"status\":\"" + status + "\"}";
    } catch (const std::exception& e) { return err(std::string("beam: ") + e.what()); }
      catch (...) { return err("beam: send failed"); }
}

std::string tx_status(const std::string& mnemonic, const std::string& dir, const std::string&, const std::string& txid_hex) {
    if (mnemonic.empty() || dir.empty() || txid_hex.empty()) return err("beam: missing args");
    beam::io::Reactor::Ptr _reactor = beam::io::Reactor::create();
    beam::io::Reactor::Scope _rscope(*_reactor);
    auto db = open_db(mnemonic, dir);
    if (!db) return err("beam: wallet db open failed");
    beam::wallet::TxID txid{};
    if (!hex_to_bytes(txid_hex, txid.data(), txid.size())) return err("beam: bad txid");
    try {
        auto tx = db->getTx(txid);                                             // VERIFY: optional<TxDescription> getTx(const TxID&)
        if (!tx) return "{\"status\":\"unknown\"}";
        return std::string("{\"status\":\"") + tx_status_str(tx->m_status) + "\"}";
    } catch (const std::exception& e) { return err(std::string("beam: ") + e.what()); }
      catch (...) { return err("beam: tx_status failed"); }
}
// Sync the wallet via QUICK SYNC only: connect to an official public BEAM node (`node_uri`) and let
// the FlyClient wallet do light, cryptographically-verified scanning of its own coins. No on-device
// node, no 127.0.0.1 — Hey talks to the public node over the network. Runs the reactor until the
// wallet finishes (UpdateCompletedAction) or a timeout, then the WalletDB reflects our coins.
std::string scan(const std::string& mnemonic, const std::string& dir, const std::string& node_uri) {
    if (mnemonic.empty() || dir.empty() || node_uri.empty()) return err("beam: missing args");
    // Re-entrancy guard: only ONE reactor at a time. If one is already running, report its state and
    // let it continue rather than starting a second.
    if (g_sync_active.exchange(true)) {
        return std::string("{\"ok\":true,\"synced\":") + (g_synced.load() ? "true" : "false") + ",\"already\":true}";
    }
    struct ActiveGuard { ~ActiveGuard() { g_sync_active = false; } } _active_guard;  // clears on every return
    g_synced = false;
    g_sync_done = 0; g_sync_total = 0;
    try {
        // Reachability gate FIRST (hard 6s deadline): a dead node / filtered
        // port now returns an honest error in seconds instead of hanging the
        // scan (and the in-flight guard) forever.
        const std::string resolved = probe_node(node_uri, 6000);
        if (resolved.empty()) {
            return err("beam: node unreachable - mobile networks often block port 8100; try Wi-Fi or another node");
        }
        // Finalize the chain Rules checksum so the FlyClient can validate genesis + headers. Idempotent.
        beam::Rules::get().UpdateChecksum();                                       // VERIFY: Rules::get().UpdateChecksum()
        beam::io::Reactor::Ptr reactor = beam::io::Reactor::create();
        beam::io::Reactor::Scope scope(*reactor);
        auto db = open_db(mnemonic, dir);
        if (!db) return err("beam: wallet db open failed");
        g_tip = db->getCurrentHeight();   // last synced tip (pre-scan) — keeps the UI honest at idle

        // Completion = the wallet finished scanning its coins against the (already-synced) public node.
        auto onUpdateComplete = []() { g_synced = true; beam::io::Reactor::get_Current().stop(); };
        auto wallet = std::make_shared<beam::wallet::Wallet>(
            db, beam::wallet::Wallet::TxCompletedAction(), onUpdateComplete);

        beam::io::Address nodeAddr;
        if (!nodeAddr.resolve(resolved.c_str())) return err("beam: bad node address");
        auto nnet = std::make_shared<beam::proto::FlyClient::NetworkStd>(*wallet);
        nnet->m_Cfg.m_vNodes.push_back(nodeAddr);
        nnet->Connect();

        auto wnet = std::make_shared<beam::wallet::WalletNetworkViaBbs>(*wallet, nnet, db);
        wallet->AddMessageEndpoint(wnet);
        wallet->SetNodeEndpoint(nnet);
        wallet->ResumeAllTransactions();

        beam::io::Timer::Ptr timer = beam::io::Timer::create(*reactor);
        timer->start(45000, false, []() { beam::io::Reactor::get_Current().stop(); });

        reactor->run();   // blocks until the wallet finishes scanning (onUpdateComplete) or the timeout
        g_tip = db->getCurrentHeight();   // the height we actually reached — shown as "block N"
        return std::string("{\"ok\":true,\"synced\":") + (g_synced.load() ? "true" : "false") +
               ",\"height\":" + std::to_string(g_tip.load()) + "}";
    } catch (const std::exception& e) { return err(std::string("beam: ") + e.what()); }
      catch (...) { return err("beam: scan failed"); }
}

// ── on-device mainnet node ────────────────────────────────────────────────────
// We reuse beam::NodeClient verbatim: it owns the node thread + io::Reactor, auto-restarts,
// and runs removeNodeDataIfNeeded() (node-DB migration auto-wipe on a BEAM-tag bump) for free.
// The wallet still uses the same FlyClient scan() path — only the node_uri is 127.0.0.1.
static std::atomic<bool>     g_node_running{false};
static std::atomic<bool>     g_node_synced{false};   // node reached tip (onStartedNode, Done==Total once)
static std::atomic<uint64_t> g_node_done{0}, g_node_total{0};
// NON-FATAL reachability hint (B3 fix). Set by the multi-seed reachability check that runs at
// node_start but NEVER aborts it. The node owns a resilient retry loop over ALL seeds (m_Connect),
// so a false here is only a UI hint ("still reaching peers"), never a veto. Updated on every start.
static std::atomic<bool>     g_peers_reachable{false};
static constexpr uint16_t    kLocalNodePort = beam::Node::s_PortDefault;  // 31744; wallet dials this

class HeyNodeObserver final : public beam::INodeClientObserver {
public:
    std::string m_dir;
    // ─ lifecycle ─
    void onNodeCreated()  override {}
    void onNodeDestroyed() override { g_node_running = false; }
    void onNodeThreadFinished() override { g_node_running = false; }
    void onStartedNode()  override { g_node_synced = true;  }   // node-synced gate (relative Done==Total, once)
    void onStoppedNode()  override { g_node_synced = false; }
    void onFailedToStartNode(beam::io::ErrorCode) override { g_node_running = false; }
    void onSyncError(beam::Node::IObserver::Error) override {}
    // ─ progress → mirror into BOTH node atomics AND the existing quick-sync UI atomics ─
    void onInitProgressUpdated(uint64_t d, uint64_t t) override {
        g_node_done = d; g_node_total = t; g_sync_done = d; g_sync_total = t;
    }
    void onSyncProgressUpdated(int d, int t) override {
        g_node_done = (uint64_t)d; g_node_total = (uint64_t)t;
        g_sync_done = (uint64_t)d; g_sync_total = (uint64_t)t;   // lights up the existing block-height bar
    }
    // ─ node config ─
    uint16_t    getLocalNodePort()    const override { return kLocalNodePort; }
    std::string getLocalNodeStorage() const override { return m_dir + "/beam-node.db"; }
    std::string getTempDir()          const override { return m_dir; }
    std::vector<std::string> getLocalNodePeers() const override {
        // ONLY the canonical resolvable mainnet seeds. BEAM's node treats an UNRESOLVABLE peer in
        // m_Connect as FATAL (io::Exception EC_EADDRNOTAVAIL → "Node stopped" → 5s restart-loop, never
        // syncs), so do NOT add guessed hostnames here — ap-nodes/eu-node01 did not resolve and
        // crash-looped the node. getDefaultPeers() (eu-nodes/us-nodes) are the verified-resolvable set.
        return beam::getDefaultPeers();
    }
    // Persistent: keep the configured seeds HOT — re-dial them every cycle instead of only once,
    // so a seed that completes TCP but stalls in the secure handshake gets retried (the no-download
    // stall fix). Without this, ActivateMorePeers() early-returns and the seeds go dormant.
    bool        getPeersPersistent()  const override { return true; }
};

// Route BEAM's internal node logs to Android logcat (tag "beam-node"). BEAM's g_logger is null by
// default, so every BEAM_LOG_* compiles to a no-op (will_log()==false) — which is why the node's
// sync state was invisible. Installing this sink lets `adb logcat -s beam-node` show the handshake,
// "Requesting block", "Peer … Tip:", timeouts, and disconnect reasons.
namespace {
class AndroidLogger : public beam::Logger {
    int _minLevel;
public:
    explicit AndroidLogger(int minLevel) : _minLevel(minLevel) { g_logger = this; }
    ~AndroidLogger() override { if (g_logger == this) g_logger = nullptr; }
    void set_header_formatter(beam::LogMessageHeaderFormatter) override {}
    void set_time_format(const char*, bool) override {}
    const FileNameType& get_current_file_name() override { static const FileNameType e; return e; }
    void rotate() override {}
    bool level_accepted(int level) override { return level >= _minLevel; }
    void write_message(const beam::LogMessageHeader& h, const char* buf, size_t size) override {
        int pr;
        switch (h.level) {
            case BEAM_LOG_LEVEL_CRITICAL:
            case BEAM_LOG_LEVEL_ERROR:   pr = ANDROID_LOG_ERROR; break;
            case BEAM_LOG_LEVEL_WARNING: pr = ANDROID_LOG_WARN;  break;
            case BEAM_LOG_LEVEL_DEBUG:   pr = ANDROID_LOG_DEBUG; break;
            default:                     pr = ANDROID_LOG_INFO;  break;
        }
        __android_log_print(pr, "beam-node", "%.*s", (int)size, buf);
    }
};
std::unique_ptr<AndroidLogger> g_beam_logger;
} // namespace

static HeyNodeObserver                    g_node_obs;   // process-lived (the node singleton points at it)
static std::unique_ptr<beam::NodeClient>  g_node;       // owns the node thread

// Start the in-process mainnet node ONCE. Owner KDF = the SAME seed the wallet uses, so the node
// tags/serves OUR UTXOs. Returns immediately; the node runs on its own thread.
//
// B3 FIX: we no longer HARD-VETO start on a single-seed pre-flight probe. That old gate (one seed,
// IPv4, 4s, no retry) tripped on mobile (cold DNS) even when peers were reachable, and falsely told
// users "port 8100 blocked" while their desktop node worked on the SAME WiFi. The node already owns
// a resilient retry loop over ALL seeds (node.m_Cfg.m_Connect), so we ALWAYS start it. We still run a
// robust NON-FATAL reachability check across EVERY current mainnet seed and stash the outcome in
// g_peers_reachable purely as a UI hint — because NodeClient swallows the "Resolved peer list is
// empty" throw (logs, no onFailedToStartNode), so without ANY signal the UI would show "running"
// forever with synced never flipping.
std::string node_start(const std::string& mnemonic, const std::string& dir) {
    if (g_node) {
        if (g_node_running.load()) return "{\"ok\":true,\"already\":true}";
        // A prior start left a DEAD node (its thread failed after construction). Returning a
        // false {"already":true} would mask the failure as "stuck syncing" forever; tear it
        // down here so the code below recreates it (and frees the loopback port for the retry).
        try { g_node->stopNode(); } catch (...) {}
        g_node.reset(); g_node_running = false; g_node_synced = false;
    }
    if (mnemonic.empty() || dir.empty()) return err("beam: missing args");
    try {
        // mainnet is the default network; arm the checksum BEFORE node init or the processor
        // throws "Data configuration is incompatible". Idempotent. (Also selects the mainnet
        // network so getDefaultPeers() below returns the mainnet seed list.)
        beam::Rules::get().UpdateChecksum();

        // Surface BEAM's node logs to logcat (`adb logcat -s beam-node`) — install once. Lets us
        // see exactly where the node stalls (handshake / tip / block requests / disconnects).
        if (!g_beam_logger) g_beam_logger.reset(new AndroidLogger(BEAM_LOG_LEVEL_INFO));

        // NON-FATAL reachability hint: probe ALL current mainnet seeds (BEAM's own getDefaultPeers()
        // — never a stale hardcoded host) with ~3s per seed and a ~10s overall cap. The result is a
        // hint only; we start the node regardless. This is the SAME list the node dials in m_Connect.
        g_peers_reachable = any_seed_reachable(beam::getDefaultPeers(), 3000, 10000);

        // Lift the master KDF from the wallet DB (needs a current reactor for the flush timer).
        // IKdf::Ptr is refcounted + self-contained (derived from the stored seed), so it outlives
        // this local reactor/db scope safely (W5).
        beam::io::Reactor::Ptr r = beam::io::Reactor::create();
        beam::io::Reactor::Scope rs(*r);
        auto db = open_db(mnemonic, dir);
        if (!db) return err("beam: wallet db open failed");
        beam::Key::IKdf::Ptr master = db->get_MasterKdf();
        if (!master) return err("beam: no master kdf");

        g_node_obs.m_dir = dir;
        g_node = std::make_unique<beam::NodeClient>(beam::Rules::get(), &g_node_obs);
        // B2: setKdf is harmless + reasonable (lets the node accelerate owned-UTXO serving), but the
        // wallet's FlyClient scans its OWN UTXOs from block data regardless — balance does NOT depend
        // on the node KDF. Kept, but not the balance mechanism.
        g_node->setKdf(master);
        g_node->start();                 // spawns the node thread + its own io::Reactor
        g_node->startNode();             // signals the thread to runLocalNode()
        g_node_running = true;
        return "{\"ok\":true}";
    } catch (const std::exception& e) { g_node.reset(); g_node_running = false; return err(std::string("beam: ") + e.what()); }
      catch (...) { g_node.reset(); g_node_running = false; return err("beam: node start failed"); }
}

// Stop + tear down the node. ~NodeClient joins the node thread (W4: Kotlin MUST call this OFF the
// main thread or it can ANR).
std::string node_stop() {
    if (g_node) { g_node->stopNode(); g_node.reset(); }
    g_node_running = false; g_node_synced = false;
    return "{\"ok\":true}";
}

// Live node status. running/synced/done/total are the existing fields; peers_reachable is the
// NON-FATAL reachability hint set at node_start (true if ANY current mainnet seed answered on TCP).
// We do NOT surface a live accessible-peer count: BEAM's get_AcessiblePeerCount() lives on the
// stack-local `Node` inside NodeClient::runLocalNode() and is not exposed via the observer or the
// NodeClient public API — reaching it would need BEAM source surgery. Instead the UI uses
// done/total>0 as the "connected / syncing" proxy (the node only reports block progress once a peer
// is feeding it blocks), plus this peers_reachable hint.
std::string node_status() {
    return std::string("{\"running\":") + (g_node_running.load() ? "true" : "false") +
           ",\"synced\":" + (g_node_synced.load() ? "true" : "false") +
           ",\"done\":"   + std::to_string(g_node_done.load()) +
           ",\"total\":"  + std::to_string(g_node_total.load()) +
           ",\"peers_reachable\":" + (g_peers_reachable.load() ? "true" : "false") + "}";
}

// Scan the wallet against the LOCAL node, gated on node-synced. Reuses scan() verbatim (only the
// node_uri is loopback). B1: while the node is still syncing (g_node_synced false up to wait_ms),
// return node_syncing:true (NOT an error) — first mainnet sync can take HOURS; Kotlin keeps polling
// nodeStatus() rather than surfacing an error.
std::string scan_local(const std::string& mnemonic, const std::string& dir, int wait_ms) {
    if (!g_node_running.load()) return err("beam: local node not started");
    int waited = 0;
    while (!g_node_synced.load() && waited < wait_ms) { usleep(250 * 1000); waited += 250; }
    if (!g_node_synced.load())
        return std::string("{\"ok\":false,\"node_syncing\":true,\"done\":") +
               std::to_string(g_node_done.load()) + ",\"total\":" + std::to_string(g_node_total.load()) + "}";
    // Node is at tip; its listener is bound. Wallet scans over loopback.
    return scan(mnemonic, dir, std::string("127.0.0.1:") + std::to_string(kLocalNodePort));
}

// Live sync snapshot (polled from Kotlin for the syncing/synced status).
std::string sync_progress() {
    uint64_t d = g_sync_done.load(), t = g_sync_total.load();
    return std::string("{\"done\":") + std::to_string(d) + ",\"total\":" + std::to_string(t) +
           ",\"active\":" + (g_sync_active.load() ? "true" : "false") +
           ",\"synced\":" + (g_synced.load() ? "true" : "false") +
           ",\"height\":" + std::to_string(g_tip.load()) + "}";
}

} // namespace heybeam

// ── JNI plumbing ──────────────────────────────────────────────────────────────
static std::string to_str(JNIEnv* env, jstring s) {
    if (!s) return {};
    const char* c = env->GetStringUTFChars(s, nullptr);
    // L-2: GetStringUTFChars can raise OutOfMemoryError and return null with a
    // pending exception. Calling further JNI ops (ReleaseStringUTFChars, NewStringUTF)
    // while an exception is pending is undefined behaviour — clear it and bail empty.
    if (env->ExceptionCheck()) { env->ExceptionClear(); if (c) env->ReleaseStringUTFChars(s, c); return {}; }
    std::string out(c ? c : "");
    if (c) env->ReleaseStringUTFChars(s, c);
    return out;
}
static jstring to_jstr(JNIEnv* env, const std::string& s) { return env->NewStringUTF(s.c_str()); }

extern "C" {

JNIEXPORT jstring JNICALL
Java_os_elastos_hey_social_BeamApi_beam_1address(JNIEnv* env, jclass, jstring mnemonic, jstring dir) {
    return to_jstr(env, heybeam::address(to_str(env, mnemonic), to_str(env, dir)));
}
JNIEXPORT jstring JNICALL
Java_os_elastos_hey_social_BeamApi_beam_1validate_1token(JNIEnv* env, jclass, jstring token) {
    return to_jstr(env, heybeam::validate_token(to_str(env, token)));
}
JNIEXPORT jstring JNICALL
Java_os_elastos_hey_social_BeamApi_beam_1balance(JNIEnv* env, jclass, jstring mnemonic, jstring dir, jstring node) {
    return to_jstr(env, heybeam::balance(to_str(env, mnemonic), to_str(env, dir), to_str(env, node)));
}
// H-1: arm the next beam_send for exactly (token, amountGroth, assetId). Called by the
// Rust hey_beam_send IMMEDIATELY before beam_send, AFTER redeem_spend + check_beam_cap.
JNIEXPORT void JNICALL
Java_os_elastos_hey_social_BeamApi_beam_1arm_1send(JNIEnv* env, jclass, jstring token, jlong amount_groth,
                                                  jint asset_id, jstring nonce) {
    arm_send(to_str(env, token), (uint64_t) amount_groth, (uint32_t) asset_id, to_str(env, nonce));
}
JNIEXPORT jstring JNICALL
Java_os_elastos_hey_social_BeamApi_beam_1send(JNIEnv* env, jclass, jstring mnemonic, jstring dir, jstring node,
                                             jstring token, jlong amount_groth, jlong fee_groth, jint asset_id,
                                             jstring nonce) {
    return to_jstr(env, heybeam::send(to_str(env, mnemonic), to_str(env, dir), to_str(env, node),
                                      to_str(env, token), (uint64_t) amount_groth, (uint64_t) fee_groth,
                                      (uint32_t) asset_id, to_str(env, nonce)));
}
JNIEXPORT jstring JNICALL
Java_os_elastos_hey_social_BeamApi_beam_1tx_1status(JNIEnv* env, jclass, jstring mnemonic, jstring dir,
                                                   jstring node, jstring txid) {
    return to_jstr(env, heybeam::tx_status(to_str(env, mnemonic), to_str(env, dir), to_str(env, node), to_str(env, txid)));
}
JNIEXPORT jstring JNICALL
Java_os_elastos_hey_social_BeamApi_beam_1scan(JNIEnv* env, jclass, jstring mnemonic, jstring dir, jstring node) {
    return to_jstr(env, heybeam::scan(to_str(env, mnemonic), to_str(env, dir), to_str(env, node)));
}
JNIEXPORT jstring JNICALL
Java_os_elastos_hey_social_BeamApi_beam_1sync_1progress(JNIEnv* env, jclass) {
    return to_jstr(env, heybeam::sync_progress());
}
// ── on-device node lifecycle ──────────────────────────────────────────────────
JNIEXPORT jstring JNICALL
Java_os_elastos_hey_social_BeamApi_beam_1node_1start(JNIEnv* env, jclass, jstring mnemonic, jstring dir) {
    return to_jstr(env, heybeam::node_start(to_str(env, mnemonic), to_str(env, dir)));
}
JNIEXPORT jstring JNICALL
Java_os_elastos_hey_social_BeamApi_beam_1node_1stop(JNIEnv* env, jclass) {
    return to_jstr(env, heybeam::node_stop());
}
JNIEXPORT jstring JNICALL
Java_os_elastos_hey_social_BeamApi_beam_1node_1status(JNIEnv* env, jclass) {
    return to_jstr(env, heybeam::node_status());
}
JNIEXPORT jstring JNICALL
Java_os_elastos_hey_social_BeamApi_beam_1scan_1local(JNIEnv* env, jclass, jstring mnemonic, jstring dir, jint waitMs) {
    return to_jstr(env, heybeam::scan_local(to_str(env, mnemonic), to_str(env, dir), (int) waitMs));
}

} // extern "C"
