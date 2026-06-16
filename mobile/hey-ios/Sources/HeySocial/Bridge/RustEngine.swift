import Foundation
#if RUST_ENGINE
import HeyEngineFFI

// The REAL engine: drives the C-ABI in include/HeyEngine.h (HeyEngine.xcframework),
// which is the SAME hey-mobile-runtime the Android app uses — so every call decodes
// the EXACT JSON shape HeyApi.kt parses. This actor is the iOS counterpart of
// `mod android` in capsules/hey-mobile-runtime/src/lib.rs.
//
// Threading: all FFI calls block (each spins a fresh current-thread Tokio runtime in
// Rust), so the whole surface runs on a single serial executor — `actor` gives us
// that for free and keeps the Sendable conformance the protocol requires.
//
// Memory: every `char*` is freed once with hey_string_free; every `uint8_t*` once
// with hey_bytes_free. The withCString helpers below guarantee the args outlive the
// call. NEVER hand a Rust pointer back to Rust except through its matching free.
//
// VERIFY(mac): a handful of call shapes (arity/JSON keys) are taken from the verified
// Android JNI map but only proven on the first `cargo build --target aarch64-apple-ios`.
// Those sites carry a `// VERIFY(mac)` note.
actor RustEngine: HeyEngine {

    // MARK: - C-string marshaling

    /// Call a C fn that returns an owned `char*`; copy it to a Swift String and free it.
    private func cstr(_ make: () -> UnsafeMutablePointer<CChar>?) -> String {
        guard let p = make() else { return "" }
        defer { hey_string_free(p) }
        return String(cString: p)
    }

    /// Marshal one Swift String into a borrowed `const char*` for the duration of `body`.
    private func with1<R>(_ a: String, _ body: (UnsafePointer<CChar>?) -> R) -> R {
        a.withCString { body($0) }
    }
    private func with2<R>(_ a: String, _ b: String, _ body: (UnsafePointer<CChar>?, UnsafePointer<CChar>?) -> R) -> R {
        a.withCString { pa in b.withCString { pb in body(pa, pb) } }
    }
    private func with3<R>(_ a: String, _ b: String, _ c: String,
                          _ body: (UnsafePointer<CChar>?, UnsafePointer<CChar>?, UnsafePointer<CChar>?) -> R) -> R {
        a.withCString { pa in b.withCString { pb in c.withCString { pc in body(pa, pb, pc) } } }
    }
    private func with4<R>(_ a: String, _ b: String, _ c: String, _ d: String,
                          _ body: (UnsafePointer<CChar>?, UnsafePointer<CChar>?, UnsafePointer<CChar>?, UnsafePointer<CChar>?) -> R) -> R {
        a.withCString { pa in b.withCString { pb in c.withCString { pc in d.withCString { pd in body(pa, pb, pc, pd) } } } }
    }
    private func with5<R>(_ a: String, _ b: String, _ c: String, _ d: String, _ e: String,
                          _ body: (UnsafePointer<CChar>?, UnsafePointer<CChar>?, UnsafePointer<CChar>?, UnsafePointer<CChar>?, UnsafePointer<CChar>?) -> R) -> R {
        a.withCString { pa in b.withCString { pb in c.withCString { pc in d.withCString { pd in e.withCString { pe in body(pa, pb, pc, pd, pe) } } } } }
    }

    /// Call a C fn that returns an owned `uint8_t*` + writes its length to out_len;
    /// copy to Data and free it. Returns nil when empty.
    private func bytes(_ make: (UnsafeMutablePointer<Int>) -> UnsafeMutablePointer<UInt8>?) -> Data? {
        var len = 0
        guard let p = withUnsafeMutablePointer(to: &len, { make($0) }), len > 0 else { return nil }
        defer { hey_bytes_free(p, len) }
        return Data(bytes: p, count: len)
    }

    // MARK: - JSON helpers (decode the engine's exact shapes)

    private static let dec = JSONDecoder()

    private func decode<T: Decodable>(_ json: String, as type: T.Type, default def: T) -> T {
        guard let data = json.data(using: .utf8),
              let v = try? Self.dec.decode(T.self, from: data) else { return def }
        return v
    }
    /// Decode but THROW the engine's `{"error":…}` (or a generic failure) — for the
    /// throwing protocol methods.
    private func decodeOrThrow<T: Decodable>(_ json: String, as type: T.Type) throws -> T {
        let data = json.data(using: .utf8) ?? Data()
        if let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
           let err = obj["error"] as? String {
            throw HeyError.engine(err)
        }
        do { return try Self.dec.decode(T.self, from: data) }
        catch { throw HeyError.engine("decode failed: \(error)") }
    }
    /// True unless the JSON is an `{"error":…}` envelope (mirrors Kotlin's `!has("error")`).
    private func ok(_ json: String) -> Bool {
        guard let data = json.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return true }
        return obj["error"] == nil
    }
    /// Throw if the JSON is an `{"error":…}` envelope, else return.
    private func ensureOk(_ json: String) throws {
        guard let data = json.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return }
        if let err = obj["error"] as? String { throw HeyError.engine(err) }
    }
    private func field(_ json: String, _ key: String) -> String {
        guard let data = json.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return "" }
        if let s = obj[key] as? String { return s }
        if let b = obj[key] as? Bool { return b ? "true" : "false" }
        if let n = obj[key] as? NSNumber { return n.stringValue }
        return ""
    }
    private func map(_ json: String) -> [String: String] {
        guard let data = json.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return [:] }
        var out: [String: String] = [:]
        for (k, v) in obj { if let s = v as? String, !s.isEmpty { out[k] = s } }
        return out
    }

    // MARK: - amount helpers (mirror HeyApi.toUnitsHex / toWeiHex)

    /// Decimal amount → smallest-units hex (no 0x) for `decimals` places. nil if unclean.
    private func unitsHex(_ amount: String, _ decimals: Int) -> String? {
        let v = NSDecimalNumber(string: amount.trimmingCharacters(in: .whitespaces))
        if v == .notANumber { return nil }
        let scaled = v.multiplying(byPowerOf10: Int16(decimals))
        // Whole-units guard: reject fractional remainders after scaling.
        let rounded = scaled.rounding(accordingToBehavior: NSDecimalNumberHandler(
            roundingMode: .down, scale: 0, raiseOnExactness: false, raiseOnOverflow: false,
            raiseOnUnderflow: false, raiseOnDivideByZero: false))
        if scaled.compare(rounded) != .orderedSame { return nil }
        // Convert the integer decimal string → base-16, no 0x.
        guard let big = BigUInt(decimal: rounded.stringValue) else { return nil }
        return big.hexString
    }
    private func weiHex(_ amount: String) -> String? { unitsHex(amount, 18) }

    // MARK: - protocol-text filter (mirror HeyApi.isProtocolText)

    private func isProtocolText(_ raw: String) -> Bool {
        let s = raw.hasPrefix("\u{0001}") ? String(raw.dropFirst()) : raw
        for p in ["hey-verse:1:", "hey-addr:1:", "hey-call:1:", "hey-del:1:", "hey-edit:1:", "hey-gcall:1:"] {
            if s.hasPrefix(p) { return true }
        }
        return false
    }
    private func shortDid(_ did: String) -> String {
        String(did.replacingOccurrences(of: "did:key:z", with: "").prefix(10)) + "…"
    }

    // ── private wire shapes (engine JSON that has no public model) ────────────

    /// One contact row from hey_contacts (HeyApi.chats()).
    private struct WireContact: Decodable {
        var did: String = ""
        var name: String = ""
        var lastPreview: String = ""
        var lastTs: Int64 = 0
        var unread: Int = 0
        var avatar: String = ""
    }
    /// One inbound call signal (hey_call_poll → [{from, payload:{type,call_id,…}}]).
    private struct WireCall: Decodable {
        var from: String = ""
        var payload: WirePayload = .init()
        struct WirePayload: Decodable { var type: String = ""; var call_id: String = "" }
    }
    /// One message-reaction row (hey_message_reactions).
    private struct WireMsgReaction: Decodable {
        var message_id: String = ""
        var emoji: String = ""
        var sender_did: String = ""
    }

    // MARK: - lifecycle / identity

    func start() async throws {
        // Install the at-rest DEK (Keychain/Secure-Enclave wrapped) BEFORE hey_start,
        // so the vault (identity seed, ratchet keys, conversation plaintext) is sealed.
        if let dek = SecureEnclaveVault.storageKeyBase64() {
            let rc = with1(dek) { hey_set_storage_key($0) }
            if rc != 0 {
                // The runtime then stays plaintext and says so (matches Android fail-loud).
                NSLog("RustEngine: hey_set_storage_key returned \(rc) — storage NOT hardware-sealed")
            }
        } else {
            NSLog("RustEngine: no usable key store — at-rest storage will be plaintext")
        }
        // Ensure the vault dir exists, then boot the runtime + carrier + receivers.
        try? FileManager.default.createDirectory(at: AppPaths.heyDir, withIntermediateDirectories: true)
        with1(AppPaths.heyDir.path) { hey_start($0) }
    }

    func restore(phrase: String) async throws {
        // Same DEK-before-boot ordering as start(), but the runtime derives the
        // identity FROM this phrase (returning-user restore / vault unseal).
        if let dek = SecureEnclaveVault.storageKeyBase64() {
            _ = with1(dek) { hey_set_storage_key($0) }
        }
        try? FileManager.default.createDirectory(at: AppPaths.heyDir, withIntermediateDirectories: true)
        // hey_restore(dir, phrase): boots start_background with identity_blob = Some(phrase).
        with2(AppPaths.heyDir.path, phrase) { d, p in hey_restore(d, p) }   // VERIFY(mac)
    }

    func whoami() async throws -> Profile {
        // whoami() → {did, ticket}; enrich with the full self-profile (nickname/bio/avatar),
        // matching how the Android app reads the did from whoami() and the rest from profile("").
        let didJson = cstr { hey_whoami() }
        let did = field(didJson, "did")
        let prof = try await profile(did: "")
        if !did.isEmpty && prof.did.isEmpty {
            return Profile(did: did)
        }
        return prof
    }

    func profile(did: String) async throws -> Profile {
        let json = with1(did) { p in cstr { hey_profile(p) } }
        return decode(json, as: Profile.self, default: Profile(did: did))
    }

    func saveProfile(nickname: String, bio: String, avatarCid: String) async throws {
        let json = with3(nickname, bio, avatarCid) { n, b, a in cstr { hey_save_profile(n, b, a) } }
        try ensureOk(json)
    }

    func recoveryPhrase() async -> String? {
        let s = cstr { hey_recovery_phrase() }
        return s.isEmpty ? nil : s
    }

    func validateMnemonic(_ phrase: String) async -> Bool {
        with1(phrase) { p in cstr { hey_validate_mnemonic(p) } } == "ok"
    }

    func friendLink() async -> String { cstr { hey_my_friend_link() } }

    func genInvite(label: String) async -> String {
        with1(label) { l in cstr { hey_gen_invite(l) } }
    }

    func acceptInvite(token: String) async throws -> String {
        let json = with1(token) { t in cstr { hey_accept_invite(t) } }
        try ensureOk(json)
        // Engine returns the new contact's did so the caller can open the chat. VERIFY(mac)
        let did = field(json, "did")
        return did
    }

    func carrierHealth() async -> CarrierHealth {
        let json = cstr { hey_carrier_health() }
        return decode(json, as: CarrierHealth.self, default: CarrierHealth())
    }

    // MARK: - feed

    func feed(limit: Int) async throws -> [Post] {
        let json = cstr { hey_feed(Int32(max(1, limit))) }
        return try decodeOrThrow(json, as: [Post].self)
    }

    func userPosts(did: String) async throws -> [Post] {
        let json = with1(did) { p in cstr { hey_user_posts(p) } }
        return decode(json, as: [Post].self, default: [])
    }

    func getPost(id: String) async throws -> Post {
        let json = with1(id) { p in cstr { hey_get_post(p) } }
        return try decodeOrThrow(json, as: Post.self)
    }

    func uploadMedia(_ data: Data, mime: String, name: String) async throws -> Media {
        let json = data.withUnsafeBytes { (raw: UnsafeRawBufferPointer) -> String in
            let base = raw.bindMemory(to: UInt8.self).baseAddress
            return with2(mime, name) { m, n in
                cstr { hey_upload_media(base, data.count, m, n) }
            }
        }
        return try decodeOrThrow(json, as: Media.self)
    }

    func createPost(caption: String, tiles: [Media]) async throws {
        let tilesData = (try? JSONEncoder().encode(tiles)) ?? Data("[]".utf8)
        let tilesJSON = String(data: tilesData, encoding: .utf8) ?? "[]"
        let json = with2(caption, tilesJSON) { c, m in cstr { hey_create_post(c, m) } }
        try ensureOk(json)
    }

    func deletePost(id: String) async throws {
        let json = with1(id) { p in cstr { hey_delete_post(p) } }
        try ensureOk(json)
    }

    func editPost(id: String, caption: String) async throws {
        let json = with2(id, caption) { i, c in cstr { hey_edit_post(i, c) } }
        try ensureOk(json)
    }

    func reactions(postId: String) async throws -> Reactions {
        let json = with1(postId) { p in cstr { hey_get_reactions(p) } }
        return try decodeOrThrow(json, as: Reactions.self)
    }

    func toggleLike(postId: String) async throws -> Reactions { try await react(postId: postId, emoji: HEY_LIKE) }

    func react(postId: String, emoji: String) async throws -> Reactions {
        let json = with2(postId, emoji) { p, e in cstr { hey_react(p, e) } }
        return try decodeOrThrow(json, as: Reactions.self)
    }

    func comments(postId: String) async throws -> [Comment] {
        let json = with1(postId) { p in cstr { hey_get_comments(p) } }
        return decode(json, as: [Comment].self, default: [])
    }

    func addComment(postId: String, text: String, parent: String) async throws -> Comment {
        let json = with3(postId, text, parent) { p, t, par in cstr { hey_add_comment(p, t, par) } }
        return try decodeOrThrow(json, as: Comment.self)
    }

    func feedRev() async -> Int64 { Int64(hey_feed_rev()) }

    // MARK: - chat

    func chats() async throws -> [Chat] {
        var out: [Chat] = []
        // contacts → 1:1 chats (preview filtered for protocol/handshake text)
        let cJson = cstr { hey_contacts() }
        for c in decode(cJson, as: [WireContact].self, default: []) {
            let preview = isProtocolText(c.lastPreview) ? "" : c.lastPreview
            out.append(Chat(id: c.did,
                            name: c.name.isEmpty ? shortDid(c.did) : c.name,
                            preview: preview, ts: c.lastTs, unread: c.unread,
                            isGroup: false, avatar: c.avatar))
        }
        // groups — count members from the raw JSON (mirror HeyApi.chats()).
        let gJson = cstr { hey_groups() }
        if let gdata = gJson.data(using: .utf8),
           let groups = try? JSONSerialization.jsonObject(with: gdata) as? [[String: Any]] {
            for o in groups {
                let members = (o["members"] as? [Any])?.count ?? 0
                let name = (o["name"] as? String) ?? ""
                out.append(Chat(id: (o["id"] as? String) ?? "",
                                name: name.isEmpty ? "Group" : name,
                                preview: "\(members) members",
                                ts: (o["lastTs"] as? NSNumber)?.int64Value ?? 0,
                                unread: (o["unread"] as? NSNumber)?.intValue ?? 0,
                                isGroup: true))
            }
        }
        return out.sorted { $0.ts > $1.ts }
    }

    func conversation(_ chat: Chat) async throws -> [Message] {
        let json = with1(chat.id) { id in
            cstr { chat.isGroup ? hey_group_conversation(id) : hey_conversation(id) }
        }
        let all = decode(json, as: [Message].self, default: [])
        // Hide protocol/handshake rows that carry no attachment (mirror HeyApi.conversation).
        return all.filter { !$0.attachments.isEmpty || !isProtocolText($0.text) }
    }

    func send(_ chat: Chat, text: String) async throws {
        let json = with2(chat.id, text) { id, t in
            cstr { chat.isGroup ? hey_send_group(id, t) : hey_send_dm(id, t) }
        }
        try ensureOk(json)
    }

    func sendAttachment(_ chat: Chat, data: Data, mime: String, name: String, text: String) async throws {
        let json = data.withUnsafeBytes { (raw: UnsafeRawBufferPointer) -> String in
            let base = raw.bindMemory(to: UInt8.self).baseAddress
            return with4(chat.id, text, mime, name) { id, t, m, n in
                cstr {
                    chat.isGroup
                        ? hey_send_group_attachment(id, t, base, data.count, m, n)
                        : hey_send_attachment(id, t, base, data.count, m, n)
                }
            }
        }
        try ensureOk(json)
    }

    func fetchAttachment(_ att: Attachment) async -> Data? {
        with1(att.raw) { a in bytes { len in hey_fetch_attachment(a, len) } }
    }

    func reactToMessage(_ chat: Chat, msgId: String, emoji: String) async throws {
        let json = with3(chat.id, msgId, emoji) { c, m, e in
            cstr { hey_react_message(c, m, e, chat.isGroup ? 1 : 0) }
        }
        try ensureOk(json)
    }

    func deleteMessage(_ chat: Chat, msgId: String) async throws {
        let json = with2(chat.id, msgId) { c, m in
            cstr { hey_delete_message(c, m, chat.isGroup ? 1 : 0) }
        }
        if field(json, "ok") != "true" { try ensureOk(json) }
    }

    func editMessage(_ chat: Chat, msgId: String, text: String) async throws {
        let json = with3(chat.id, msgId, text) { c, m, t in
            cstr { hey_edit_message(c, m, t, chat.isGroup ? 1 : 0) }
        }
        if field(json, "ok") != "true" { try ensureOk(json) }
    }

    func messageReactions(_ chat: Chat) async -> [String: [MsgReaction]] {
        let json = with1(chat.id) { id in cstr { hey_message_reactions(id, chat.isGroup ? 1 : 0) } }
        let rows = decode(json, as: [WireMsgReaction].self, default: [])
        var out: [String: [MsgReaction]] = [:]
        for r in rows {
            out[r.message_id, default: []].append(
                MsgReaction(messageId: r.message_id, emoji: r.emoji, sender: r.sender_did))
        }
        return out
    }

    func createGroup(name: String, members: [String]) async throws -> String {
        let membersData = (try? JSONEncoder().encode(members)) ?? Data("[]".utf8)
        let membersJSON = String(data: membersData, encoding: .utf8) ?? "[]"
        let json = with2(name, membersJSON) { n, m in cstr { hey_create_group(n, m) } }
        try ensureOk(json)
        return field(json, "id")
    }

    func startChat(did: String) async throws {
        let json = with1(did) { d in cstr { hey_start_chat(d) } }
        try ensureOk(json)
    }

    func deleteChat(_ chat: Chat) async {
        _ = with1(chat.id) { id in
            cstr { chat.isGroup ? hey_delete_group(id) : hey_delete_conversation(id) }
        }
    }

    func markRead(did: String) async { with1(did) { d in hey_mark_read(d) } }

    func totalUnread() async -> Int { Int(hey_total_unread()) }

    func peerTicket(did: String) async -> String { with1(did) { d in cstr { hey_peer_ticket(d) } } }

    // MARK: - social graph

    func follow(_ input: String) async throws {
        let json = with1(input) { i in cstr { hey_follow(i) } }
        try ensureOk(json)
    }

    func unfollow(did: String) async throws {
        let json = with1(did) { d in cstr { hey_unfollow(d) } }
        try ensureOk(json)
    }

    func following() async throws -> [Follow] {
        let json = cstr { hey_following() }
        return decode(json, as: [Follow].self, default: [])
    }

    func followers() async throws -> [Follow] {
        let json = cstr { hey_followers() }
        return decode(json, as: [Follow].self, default: [])
    }

    func followBack(did: String) async throws {
        let json = with1(did) { d in cstr { hey_follow_back(d) } }
        try ensureOk(json)
    }

    func isFollowing(did: String) async -> Bool {
        let json = with1(did) { d in cstr { hey_is_following(d) } }
        return field(json, "following") == "true"
    }

    func userProfile(did: String) async throws -> UserProfile {
        // Engine has no single UserProfile shape — aggregate like the Android UserProfileScreen:
        // get_profile + user_posts.count + is_following. Per-DID follower/following counts
        // are not exposed by the engine, so they stay 0 (matches Android, which shows neither).
        let prof = try await profile(did: did)
        let postCount = (try? await userPosts(did: did).count) ?? 0
        let following = await isFollowing(did: did)
        return UserProfile(did: prof.did.isEmpty ? did : prof.did,
                           nickname: prof.nickname, bio: prof.bio, avatar: prof.avatar,
                           followers: 0, following: 0, posts: postCount, isFollowing: following)
    }

    func drainNotifs() async -> [HeyNotification] {
        let json = cstr { hey_drain_notifs() }
        return decode(json, as: [HeyNotification].self, default: [])
    }

    // MARK: - wallet
    //
    // The runtime is always up on iOS, so we pass "" for the mnemonic and Rust
    // resolves the runtime-held seed in-process (guard.rs: secrets used, never owned).
    // MONEY: authorize…Send mints a one-shot grant; walletSend(auth:) redeems it.

    private let runtimePhrase = ""   // "" = resolve the runtime-held seed in Rust

    func walletAddress() async -> String? {
        let s = with1(runtimePhrase) { m in cstr { hey_wallet_address(m) } }
        return s.isEmpty ? nil : s
    }

    func elastosDid() async -> String? {
        let s = with1(runtimePhrase) { m in cstr { hey_elastos_did(m) } }
        return s.isEmpty ? nil : s
    }

    func elaAddress() async -> String? {
        let s = with1(runtimePhrase) { m in cstr { hey_ela_address(m) } }
        return s.isEmpty ? nil : s
    }

    func walletChains() async -> [ChainInfo] {
        let json = cstr { hey_wallet_chains() }
        return decode(json, as: [ChainInfo].self, default: [])
    }

    func walletBalance(chain: String) async -> WalletInfo? {
        let json = with2(runtimePhrase, chain) { m, c in cstr { hey_wallet_balance(m, c) } }
        guard ok(json) else { return nil }
        return decode(json, as: WalletInfo.self, default: WalletInfo())
    }

    func balances(chain: String, includeHidden: Bool) async -> [TokenBal] {
        // hey_wallet_balances → {address, tokens:[…]}. includeHidden filtering is a
        // local pref on Android; iOS has no hidden-token store yet → return all.
        let json = with2(runtimePhrase, chain) { m, c in cstr { hey_wallet_balances(m, c) } }
        guard let data = json.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let tokens = obj["tokens"] else { return [] }
        guard let tdata = try? JSONSerialization.data(withJSONObject: tokens) else { return [] }
        return (try? Self.dec.decode([TokenBal].self, from: tdata)) ?? []
    }

    func elaBalance() async -> String? {
        let json = with1(runtimePhrase) { m in cstr { hey_ela_balance(m) } }
        guard ok(json) else { return nil }
        let ela = field(json, "ela")
        return ela.isEmpty ? nil : ela
    }

    func checkAddress(_ addr: String) async throws -> String {
        let json = with1(addr.trimmingCharacters(in: .whitespaces)) { a in cstr { hey_wallet_check_address(a) } }
        if field(json, "ok") == "true" { return field(json, "address") }
        let err = field(json, "error")
        throw HeyError.engine(err.isEmpty ? "Invalid address" : err)
    }

    func txStatus(chain: String, hash: String) async -> String {
        let json = with2(chain, hash) { c, h in cstr { hey_wallet_tx_status(c, h) } }
        let s = field(json, "status")
        return s.isEmpty ? "pending" : s
    }

    func authorizeEvmSend(chain: String, to: String, amount: String) async -> SpendGrant {
        guard let wei = weiHex(amount) else { return "" }
        return mintSpend(kind: "evm:\(chain)", to: to.trimmingCharacters(in: .whitespaces), amount: wei)
    }

    func authorizeTokenSend(chain: String, contract: String, to: String, amount: String, decimals: Int) async -> SpendGrant {
        guard let units = unitsHex(amount, decimals) else { return "" }
        return mintSpend(kind: "erc20:\(chain):\(contract)", to: to.trimmingCharacters(in: .whitespaces), amount: units)
    }

    func authorizeElaSend(to: String, amount: String) async -> SpendGrant {
        mintSpend(kind: "ela", to: to.trimmingCharacters(in: .whitespaces), amount: amount.trimmingCharacters(in: .whitespaces))
    }

    private func mintSpend(kind: String, to: String, amount: String) -> SpendGrant {
        let json = with3(kind, to, amount) { k, t, a in cstr { hey_authorize_spend(k, t, a) } }
        return field(json, "token")
    }

    func walletSend(chain: String, to: String, amount: String, auth: SpendGrant) async throws -> String {
        guard let wei = weiHex(amount) else { throw HeyError.engine("Invalid amount") }
        let json = with5(runtimePhrase, chain, to.trimmingCharacters(in: .whitespaces), wei, auth) { m, c, t, v, au in
            cstr { hey_wallet_send(m, c, t, v, au) }
        }
        try ensureOk(json)
        return field(json, "txHash")
    }

    func tokenSend(chain: String, contract: String, to: String, amount: String, decimals: Int, auth: SpendGrant) async throws -> String {
        guard let units = unitsHex(amount, decimals) else { throw HeyError.engine("Invalid amount") }
        // 6 args → marshal the two static-ish ones via nested withCString.
        let json = runtimePhrase.withCString { m in
            chain.withCString { c in
                contract.withCString { ct in
                    to.trimmingCharacters(in: .whitespaces).withCString { t in
                        units.withCString { a in
                            auth.withCString { au in
                                self.cstr { hey_wallet_token_send(m, c, ct, t, a, au) }
                            }
                        }
                    }
                }
            }
        }
        try ensureOk(json)
        return field(json, "txHash")
    }

    func elaSend(to: String, amount: String, auth: SpendGrant) async throws -> String {
        let json = with4(runtimePhrase, to.trimmingCharacters(in: .whitespaces), amount.trimmingCharacters(in: .whitespaces), auth) { m, t, a, au in
            cstr { hey_ela_send(m, t, a, au) }
        }
        try ensureOk(json)
        return field(json, "txHash")
    }

    func txHistory() async -> [TxRecord] {
        // Local-only history (Android stores it in SharedPreferences). iOS keeps it in
        // UserDefaults under the App Group so the wallet sheet shows what we sent.
        guard let data = Self.defaults?.data(forKey: Self.txKey),
              let recs = try? Self.dec.decode([TxRecord].self, from: data) else { return [] }
        return recs.sorted { $0.ts > $1.ts }
    }

    func recordTx(_ tx: TxRecord) async {
        var recs = await txHistory()
        recs.insert(tx, at: 0)
        if recs.count > 200 { recs = Array(recs.prefix(200)) }
        if let data = try? JSONEncoder().encode(recs) { Self.defaults?.set(data, forKey: Self.txKey) }
    }

    func auditLog(limit: Int) async -> String {
        cstr { hey_audit_log(Int32(max(1, limit))) }
    }

    private static let defaults = UserDefaults(suiteName: AppPaths.appGroup)
    private static let txKey = "hey.tx_history"

    // MARK: - BEAM (absent from this build — mirror MockEngine's "not in this build")

    nonisolated var beamAvailable: Bool { false }   // libbeam (C++) is not cross-compiled for iOS (see HEY_IOS_PORT_PLAN)
    func beamAddress() async -> String? { nil }
    func beamBalance() async -> BeamBalance? { nil }
    func beamScan() async -> BeamScanResult { BeamScanResult(error: "BEAM not in this build") }
    func beamSend(token: String, amount: String, asset: Int) async throws -> BeamSendResult {
        throw HeyError.engine("BEAM not in this build")
    }

    // MARK: - tipping

    func resolveTip(did: String) async -> [String: String] {
        map(with1(did) { d in cstr { hey_resolve_tip(d) } })
    }

    func refreshContact(did: String) async -> [String: String] {
        map(with1(did) { d in cstr { hey_refresh_contact(d) } })
    }

    func publishTipAddresses() async -> Bool {
        // Build {chainKey:address}: EVM 0x for every EVM chain + ELA E… (BEAM absent on iOS).
        guard let evm = await walletAddress() else { return false }
        var addrs: [String: String] = [:]
        for c in await walletChains() { addrs[c.key] = evm }
        if let ela = await elaAddress() { addrs["ela"] = ela }
        guard let data = try? JSONSerialization.data(withJSONObject: addrs),
              let body = String(data: data, encoding: .utf8) else { return false }
        let json = with1(body) { b in cstr { hey_set_tip_addresses(b) } }
        return ok(json)
    }

    func notifyTip(to: String, symbol: String, amount: String, txHash: String) async {
        _ = with4(to, symbol, amount, txHash) { t, s, a, h in cstr { hey_notify_tip(t, s, a, h) } }
    }

    // MARK: - verse lane

    func verseSend(did: String, payloadJSON: String) async throws -> Bool {
        let json = with2(did, payloadJSON) { d, p in cstr { hey_verse_send(d, p) } }
        return field(json, "ok") == "true"
    }

    func versePoll() async -> String { cstr { hey_verse_poll() } }

    // MARK: - 1:1 voice calls

    func callSend(did: String, payloadJSON: String) async -> Bool {
        let json = with2(did, payloadJSON) { d, p in cstr { hey_call_send(d, p) } }
        return field(json, "ok") == "true"
    }

    func callPoll() async -> [CallSignal] {
        let json = cstr { hey_call_poll() }
        // Re-shape {from, payload:{type,call_id}} → CallSignal, keeping the raw payload JSON.
        guard let data = json.data(using: .utf8),
              let arr = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]] else { return [] }
        return arr.compactMap { o in
            guard let payload = o["payload"] as? [String: Any] else { return nil }
            let raw = (try? JSONSerialization.data(withJSONObject: payload)).flatMap { String(data: $0, encoding: .utf8) } ?? "{}"
            return CallSignal(from: o["from"] as? String ?? "",
                              type: payload["type"] as? String ?? "",
                              callId: payload["call_id"] as? String ?? "",
                              payloadJSON: raw)
        }
    }

    func voiceStart(peerTicket: String, isCaller: Bool) async {
        with1(peerTicket) { t in hey_voice_start(t, isCaller ? 1 : 0) }
    }

    func voicePeers() async -> Int { Int(hey_voice_peers()) }

    func voiceSend(_ pcm: Data) async {
        pcm.withUnsafeBytes { (raw: UnsafeRawBufferPointer) in
            if let base = raw.bindMemory(to: UInt8.self).baseAddress {
                hey_voice_send(base, pcm.count)
            }
        }
    }

    func voiceRecv(maxBytes: Int) async -> Data {
        bytes { len in hey_voice_recv(Int32(max(0, maxBytes)), len) } ?? Data()
    }

    func voiceSetMuted(_ muted: Bool) async { hey_voice_set_muted(muted ? 1 : 0) }

    func voiceStop() async { hey_voice_stop() }

    // MARK: - content + network

    func content(cid: String) async -> Data? {
        with1(cid) { c in bytes { len in hey_content_bytes(c, len) } }
    }

    nonisolated func netChanged() { hey_net_changed() }
}

// MARK: - minimal big-integer for decimal → hex (no Foundation BigInt on iOS 16)

/// Just enough unsigned big-int to turn an integer DECIMAL string into a base-16
/// string with no 0x — the exact shape `hey_authorize_spend`/`hey_wallet_send`
/// expect (mirrors HeyApi.toUnitsHex's BigInteger.toString(16)).
private struct BigUInt {
    private var words: [UInt32] = [0]   // little-endian base-2^32 limbs

    init?(decimal: String) {
        let s = decimal.trimmingCharacters(in: .whitespaces)
        guard !s.isEmpty, s.allSatisfy({ $0.isNumber }) else { return nil }
        for ch in s {
            let d = UInt32(ch.wholeNumberValue ?? 0)
            mulAdd(10, d)
        }
        trim()
    }

    private mutating func mulAdd(_ m: UInt32, _ add: UInt32) {
        var carry: UInt64 = UInt64(add)
        for i in 0..<words.count {
            let cur = UInt64(words[i]) * UInt64(m) + carry
            words[i] = UInt32(truncatingIfNeeded: cur)
            carry = cur >> 32
        }
        while carry > 0 {
            words.append(UInt32(truncatingIfNeeded: carry))
            carry >>= 32
        }
    }

    private mutating func trim() {
        while words.count > 1 && words.last == 0 { words.removeLast() }
    }

    var hexString: String {
        var out = String(words[words.count - 1], radix: 16)
        if words.count >= 2 {
            for i in stride(from: words.count - 2, through: 0, by: -1) {
                out += String(format: "%08x", words[i])
            }
        }
        return out
    }
}
#endif
