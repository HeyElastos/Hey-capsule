import Foundation

// Fake data so the SwiftUI app builds and runs in the simulator with NO native
// dependency — iterate on look-and-feel before HeyEngine.xcframework exists.
// Implements the FULL HeyEngine contract; swap HeyEngineFactory.live to RustEngine
// (RUST_ENGINE flag) once the xcframework is built. Delete nothing here.

actor MockEngine: HeyEngine {
    private let me = Profile(did: "did:key:z6MkMockSelf000000000000000000000000000aa",
                             nickname: "You", bio: "sovereign · post-quantum", avatar: "")
    private var posts: [Post] = []
    private var chatList: [Chat] = []
    private var threads: [String: [Message]] = [:]
    private var reactionState: [String: Reactions] = [:]
    private var commentState: [String: [Comment]] = [:]
    private var followingSet: Set<String> = ["did:key:zAlice"]

    private static func now() -> Int64 { Int64(Date().timeIntervalSince1970 * 1000) }
    private func uid(_ p: String = "id") -> String { "\(p)-\(UUID().uuidString.prefix(8))" }

    init() {
        let now = Self.now()
        chatList = [
            Chat(id: "did:key:zAlice", name: "Alice", preview: "see you in the verse 🌆", ts: now - 60_000, unread: 2, online: true),
            Chat(id: "did:key:zBob", name: "Bob", preview: "sent you 5 ELA", ts: now - 3_600_000),
            Chat(id: "grp:design", name: "Design", preview: "3 members", ts: now - 7_200_000, unread: 1, isGroup: true),
        ]
        threads["did:key:zAlice"] = [
            Message(id: "m1", text: "hey! you on iOS now?", ts: now - 120_000),
            Message(id: "m2", text: "yeah — same engine, native UI", ts: now - 90_000, mine: true),
            Message(id: "m3", text: "see you in the verse 🌆", ts: now - 60_000),
        ]
        posts = [
            Post(id: "p1", author: "did:key:zAlice", authorName: "Alice",
                 caption: "first post from the iPhone build ✨", ts: now - 300_000,
                 media: [Media(cid: "mock-1", mime: "image/jpeg"), Media(cid: "mock-2", mime: "image/jpeg")]),
            Post(id: "p2", author: me.did, authorName: me.nickname,
                 caption: "one seed, all chains — DID + ELA + ESC + BEAM", ts: now - 900_000),
        ]
        reactionState["p1"] = Reactions(counts: [HEY_LIKE: 12], mine: HEY_LIKE, total: 12)
        reactionState["p2"] = Reactions(counts: [HEY_LIKE: 7], total: 7)
        commentState["p1"] = [Comment(id: "c1", author: "did:key:zBob", authorName: "Bob", text: "clean 🔥", ts: now - 60_000)]
    }

    // MARK: lifecycle / identity
    func start() async throws {}
    func restore(phrase: String) async throws {}
    func whoami() async throws -> Profile { me }
    func profile(did: String) async throws -> Profile {
        if did.isEmpty || did == me.did { return me }
        return Profile(did: did, nickname: chatList.first { $0.id == did }?.name ?? Profile.short(did), bio: "on Hey")
    }
    func saveProfile(nickname: String, bio: String, avatarCid: String) async throws {}
    func recoveryPhrase() async -> String? { "abandon ability able about above absent absorb abstract absurd abuse access accident" }
    func validateMnemonic(_ phrase: String) async -> Bool { phrase.split(separator: " ").count >= 12 }
    func friendLink() async -> String { "hey-invite:1:mock\(me.did)" }
    func genInvite(label: String) async -> String { "hey-invite:1:mock-\(label)" }
    func acceptInvite(token: String) async throws -> String {
        let did = "did:key:zInvited\(UUID().uuidString.prefix(6))"
        try await startChat(did: did)
        return did
    }
    func carrierHealth() async -> CarrierHealth { CarrierHealth(online: true, peers: 3, relay: "elastos.app", direct: true) }

    // MARK: feed
    func feed(limit: Int) async throws -> [Post] { posts }
    func userPosts(did: String) async throws -> [Post] { posts.filter { $0.author == did } }
    func getPost(id: String) async throws -> Post {
        guard let p = posts.first(where: { $0.id == id }) else { throw HeyError.engine("no such post") }
        return p
    }
    func uploadMedia(_ data: Data, mime: String, name: String) async throws -> Media {
        Media(cid: uid("cid"), mime: mime, type: mime.hasPrefix("video/") ? "video" : "photo", name: name)
    }
    func createPost(caption: String, tiles: [Media]) async throws {
        let p = Post(id: uid("p"), author: me.did, authorName: me.nickname, caption: caption, ts: Self.now(), media: tiles)
        posts.insert(p, at: 0)
        reactionState[p.id] = .empty
    }
    func deletePost(id: String) async throws { posts.removeAll { $0.id == id } }
    func editPost(id: String, caption: String) async throws {
        if let i = posts.firstIndex(where: { $0.id == id }) { posts[i].caption = caption }
    }
    func reactions(postId: String) async throws -> Reactions { reactionState[postId] ?? .empty }
    func toggleLike(postId: String) async throws -> Reactions { try await react(postId: postId, emoji: HEY_LIKE) }
    func react(postId: String, emoji: String) async throws -> Reactions {
        var r = reactionState[postId] ?? .empty
        if r.mine == emoji { r.mine = ""; r.counts[emoji, default: 1] -= 1; r.total -= 1 }
        else { if !r.mine.isEmpty { r.counts[r.mine, default: 1] -= 1; r.total -= 1 }; r.mine = emoji; r.counts[emoji, default: 0] += 1; r.total += 1 }
        r.counts = r.counts.filter { $0.value > 0 }
        reactionState[postId] = r
        return r
    }
    func comments(postId: String) async throws -> [Comment] { commentState[postId] ?? [] }
    func addComment(postId: String, text: String, parent: String) async throws -> Comment {
        let c = Comment(id: uid("c"), author: me.did, authorName: me.nickname, text: text, ts: Self.now(), parent: parent)
        commentState[postId, default: []].append(c)
        return c
    }
    func feedRev() async -> Int64 { Int64(posts.count) }

    // MARK: chat
    func chats() async throws -> [Chat] { chatList.sorted { $0.ts > $1.ts } }
    func conversation(_ chat: Chat) async throws -> [Message] { threads[chat.id] ?? [] }
    func send(_ chat: Chat, text: String) async throws {
        let now = Self.now()
        threads[chat.id, default: []].append(Message(id: uid("m"), text: text, ts: now, mine: true))
        if let i = chatList.firstIndex(where: { $0.id == chat.id }) { chatList[i].preview = text; chatList[i].ts = now }
    }
    func sendAttachment(_ chat: Chat, data: Data, mime: String, name: String, text: String) async throws {
        let att = Attachment(name: name, mime: mime, size: Int64(data.count))
        threads[chat.id, default: []].append(Message(id: uid("m"), text: text, ts: Self.now(), mine: true, attachments: [att]))
    }
    func fetchAttachment(_ att: Attachment) async -> Data? { nil }
    func reactToMessage(_ chat: Chat, msgId: String, emoji: String) async throws {}
    func deleteMessage(_ chat: Chat, msgId: String) async throws { threads[chat.id]?.removeAll { $0.id == msgId } }
    func editMessage(_ chat: Chat, msgId: String, text: String) async throws {
        if let i = threads[chat.id]?.firstIndex(where: { $0.id == msgId }) { threads[chat.id]?[i].text = text }
    }
    func messageReactions(_ chat: Chat) async -> [String: [MsgReaction]] { [:] }
    func createGroup(name: String, members: [String]) async throws -> String {
        let id = "grp:\(uid())"
        chatList.insert(Chat(id: id, name: name, preview: "\(members.count + 1) members", ts: Self.now(), isGroup: true), at: 0)
        return id
    }
    func startChat(did: String) async throws {
        if !chatList.contains(where: { $0.id == did }) {
            chatList.insert(Chat(id: did, name: Profile.short(did), ts: Self.now()), at: 0)
        }
    }
    func deleteChat(_ chat: Chat) async { chatList.removeAll { $0.id == chat.id }; threads[chat.id] = nil }
    func markRead(did: String) async { if let i = chatList.firstIndex(where: { $0.id == did }) { chatList[i].unread = 0 } }
    func totalUnread() async -> Int { chatList.reduce(0) { $0 + $1.unread } }
    func peerTicket(did: String) async -> String { "mock-ticket-\(did.suffix(6))" }

    // MARK: social graph
    func follow(_ input: String) async throws { followingSet.insert(input) }
    func unfollow(did: String) async throws { followingSet.remove(did) }
    func following() async throws -> [Follow] { followingSet.map { Follow(did: $0) } }
    func followers() async throws -> [Follow] { [Follow(did: "did:key:zBob"), Follow(did: "did:key:zAlice")] }
    func followBack(did: String) async throws { followingSet.insert(did) }
    func isFollowing(did: String) async -> Bool { followingSet.contains(did) }
    func userProfile(did: String) async throws -> UserProfile {
        let p = try await profile(did: did)
        return UserProfile(did: p.did, nickname: p.displayName, bio: p.bio.isEmpty ? "on Hey" : p.bio,
                           avatar: p.avatar, followers: 42, following: 17, posts: posts.filter { $0.author == did }.count,
                           isFollowing: followingSet.contains(did))
    }
    func drainNotifs() async -> [HeyNotification] {
        let now = Self.now()
        return [
            HeyNotification(kind: "like", did: "did:key:zAlice", name: "Alice", text: "liked your post", ts: now - 120_000, postId: "p2"),
            HeyNotification(kind: "follow", did: "did:key:zBob", name: "Bob", text: "started following you", ts: now - 3_600_000),
            HeyNotification(kind: "tip", did: "did:key:zAlice", name: "Alice", text: "tipped you 1 ELA", ts: now - 7_200_000),
        ]
    }

    // MARK: wallet
    func walletAddress() async -> String? { "0xMockEsc0000000000000000000000000000000000" }
    func elastosDid() async -> String? { "did:elastos:iMockEid00000000000000000000000000" }
    func elaAddress() async -> String? { "EaBcMockEla0000000000000000000000000" }
    func walletChains() async -> [ChainInfo] {
        [ChainInfo(key: "esc", name: "Elastos Smart Chain", chainId: 20, symbol: "ELA"),
         ChainInfo(key: "ethereum", name: "Ethereum", chainId: 1, symbol: "ETH")]
    }
    func walletBalance(chain: String) async -> WalletInfo? {
        WalletInfo(address: "0xMockEsc0000000000000000000000000000000000", balance: "3.0", wei: "0", symbol: chain == "ethereum" ? "ETH" : "ELA")
    }
    func balances(chain: String, includeHidden: Bool) async -> [TokenBal] {
        [TokenBal(symbol: chain == "ethereum" ? "ETH" : "ELA", name: "Native", decimals: 18, native: true, balance: "3.0"),
         TokenBal(symbol: "SAIL", name: "Glide", contract: "0xToken", decimals: 18, balance: "120.0")]
    }
    func elaBalance() async -> String? { "12.5" }
    func checkAddress(_ addr: String) async throws -> String {
        guard addr.count > 8 else { throw HeyError.engine("Invalid address") }
        return addr
    }
    func txStatus(chain: String, hash: String) async -> String { "success" }
    func authorizeEvmSend(chain: String, to: String, amount: String) async -> SpendGrant { "mock-grant-\(UUID().uuidString.prefix(8))" }
    func authorizeTokenSend(chain: String, contract: String, to: String, amount: String, decimals: Int) async -> SpendGrant { "mock-grant-\(UUID().uuidString.prefix(8))" }
    func authorizeElaSend(to: String, amount: String) async -> SpendGrant { "mock-grant-\(UUID().uuidString.prefix(8))" }
    func walletSend(chain: String, to: String, amount: String, auth: SpendGrant) async throws -> String { "0xmock\(UUID().uuidString.prefix(12))" }
    func tokenSend(chain: String, contract: String, to: String, amount: String, decimals: Int, auth: SpendGrant) async throws -> String { "0xmock\(UUID().uuidString.prefix(12))" }
    func elaSend(to: String, amount: String, auth: SpendGrant) async throws -> String { "mocktx\(UUID().uuidString.prefix(12))" }
    func txHistory() async -> [TxRecord] {
        [TxRecord(chain: "esc", symbol: "ELA", to: "0xBob", amount: "1.0", hash: "0xmocktx0001", kind: "sent", ts: Self.now() - 86_400_000)]
    }
    func recordTx(_ tx: TxRecord) async {}
    func auditLog(limit: Int) async -> String { "[]" }

    // MARK: BEAM
    nonisolated var beamAvailable: Bool { false }
    func beamAddress() async -> String? { nil }
    func beamBalance() async -> BeamBalance? { nil }
    func beamScan() async -> BeamScanResult { BeamScanResult(error: "BEAM not in this build") }
    func beamSend(token: String, amount: String, asset: Int) async throws -> BeamSendResult { throw HeyError.engine("BEAM not in this build") }

    // MARK: tipping
    func resolveTip(did: String) async -> [String: String] { ["esc": "0xBob", "ela": "EBobMock"] }
    func refreshContact(did: String) async -> [String: String] { await resolveTip(did: did) }
    func publishTipAddresses() async -> Bool { true }
    func notifyTip(to: String, symbol: String, amount: String, txHash: String) async {}

    // MARK: verse
    func verseSend(did: String, payloadJSON: String) async throws -> Bool { true }
    func versePoll() async -> String { "[]" }

    // MARK: voice
    func callSend(did: String, payloadJSON: String) async -> Bool { true }
    func callPoll() async -> [CallSignal] { [] }
    func voiceStart(peerTicket: String, isCaller: Bool) async {}
    func voicePeers() async -> Int { 1 }
    func voiceSend(_ pcm: Data) async {}
    func voiceRecv(maxBytes: Int) async -> Data { Data() }
    func voiceSetMuted(_ muted: Bool) async {}
    func voiceStop() async {}

    // MARK: content + network
    func content(cid: String) async -> Data? { nil }   // no real bytes → placeholder renders
    nonisolated func netChanged() {}
}
