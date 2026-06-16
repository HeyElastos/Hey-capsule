// Hey's OWN thin BEAM shim interface (not vendored from android-wallet).
// Stateless re: the SEED — the mnemonic is passed per call. BEAM inherently needs a persistent
// WalletDB on disk (it holds coins + sync state), so `dir` is the app's private dir; the DB is
// opened/created from the seed with a seed-derived password. All functions return a JSON string
// ({...} or {"error":"..."}). The UI gates `send` behind biometric auth and calls these off-main.
//
// Phase A (LOCAL, no node): address + validate_token are implemented — they give the receive side
// (the public_offline token Hey publishes for elastos://beam/ namespace tipping).
// Phase B (node + reactor): balance/scan/send remain stubbed until the FlyClient reactor is wired.
//
// Money-safety: send stays disabled until the gate in docs/HEY_BEAM_INTEGRATION.md passes.
#pragma once
#include <string>
#include <cstdint>

namespace heybeam {

// Mint/return the static public_offline ("donation") token to publish for tipping. LOCAL.
// Returns {"token":"<base58>"} or {"error"}.
std::string address(const std::string& mnemonic, const std::string& dir);

// Validate a recipient token: {"valid":true,"type":"public_offline"} or {"valid":false}. LOCAL.
std::string validate_token(const std::string& token);

// Spendable + maturing balance from the (last-synced) WalletDB. {"available","maturing"} or {"error"}.
std::string balance(const std::string& mnemonic, const std::string& dir, const std::string& node_uri);

// Build + broadcast a payment to a recipient token. asset_id: 0 = BEAM, 7 = BEAMX (same wallet/
// address — the asset is a tx-level field). {"txid","status"} or {"error"}.
// H1-1: `nonce` is the FRESH single-use key the Rust caller armed this send with — send() consumes
// the matching arm keyed on the nonce, so each arm authorizes exactly one send.
std::string send(const std::string& mnemonic, const std::string& dir, const std::string& node_uri,
                 const std::string& token, uint64_t amount_groth, uint64_t fee_groth, uint32_t asset_id,
                 const std::string& nonce);

// {"status":"pending|confirmed|failed"} or {"error"}.
std::string tx_status(const std::string& mnemonic, const std::string& dir, const std::string& node_uri,
                      const std::string& txid);

// Connect + sync so received (public)offline coins become visible. {"ok":true} or {"error"}.
std::string scan(const std::string& mnemonic, const std::string& dir, const std::string& node_uri);

} // namespace heybeam
