import Foundation

// The seam between SwiftUI and the Rust engine (capsules/hey-mobile-runtime).
//
// This protocol is the LOCKED CONTRACT. It mirrors HeyApi.kt's `external fun` +
// helper surface 1:1, expressed as an async Swift API. Two implementations:
//   • MockEngine  — fake data, no native dependency. The DEFAULT so the whole UI
//                   builds and runs in the simulator before the xcframework exists.
//   • RustEngine  — calls the C-ABI in include/HeyEngine.h (built into
//                   HeyEngine.xcframework). Compiled in only when RUST_ENGINE is set.
//
// Same engine as Android, byte-for-byte — so every return type decodes the exact
// JSON the Android app parses. Adding a screen? Add its method here, in MockEngine,
// and in RustEngine (+ the C export in ios.rs / HeyEngine.h) — never call native
// from a View directly.

enum HeyError: Error, LocalizedError {
    case engine(String)
    case notReady
    var errorDescription: String? {
        switch self {
        case .engine(let m): return m
        case .notReady: return "The runtime isn't ready yet."
        }
    }
}

protocol HeyEngine: Sendable {
    // MARK: lifecycle / identity
    func start() async throws
    /// Boot the runtime deriving the identity FROM this BIP39 phrase (returning-user
    /// restore, or a vault unseal that recovered the seed). Call INSTEAD of start().
    func restore(phrase: String) async throws
    func whoami() async throws -> Profile
    func profile(did: String) async throws -> Profile            // "" = self
    func saveProfile(nickname: String, bio: String, avatarCid: String) async throws
    func recoveryPhrase() async -> String?
    func validateMnemonic(_ phrase: String) async -> Bool
    func friendLink() async -> String
    func genInvite(label: String) async -> String
    /// Accept a hey-invite token. Returns the new contact's did ("" if the engine
    /// didn't surface one) so the caller can open the chat immediately.
    func acceptInvite(token: String) async throws -> String
    func carrierHealth() async -> CarrierHealth

    // MARK: feed
    func feed(limit: Int) async throws -> [Post]
    func userPosts(did: String) async throws -> [Post]
    func getPost(id: String) async throws -> Post
    func uploadMedia(_ data: Data, mime: String, name: String) async throws -> Media
    func createPost(caption: String, tiles: [Media]) async throws
    func deletePost(id: String) async throws
    func editPost(id: String, caption: String) async throws
    func reactions(postId: String) async throws -> Reactions
    func toggleLike(postId: String) async throws -> Reactions
    func react(postId: String, emoji: String) async throws -> Reactions
    func comments(postId: String) async throws -> [Comment]
    func addComment(postId: String, text: String, parent: String) async throws -> Comment
    func feedRev() async -> Int64

    // MARK: chat
    func chats() async throws -> [Chat]
    func conversation(_ chat: Chat) async throws -> [Message]
    func send(_ chat: Chat, text: String) async throws
    func sendAttachment(_ chat: Chat, data: Data, mime: String, name: String, text: String) async throws
    func fetchAttachment(_ att: Attachment) async -> Data?
    func reactToMessage(_ chat: Chat, msgId: String, emoji: String) async throws
    func deleteMessage(_ chat: Chat, msgId: String) async throws
    func editMessage(_ chat: Chat, msgId: String, text: String) async throws
    func messageReactions(_ chat: Chat) async -> [String: [MsgReaction]]
    func createGroup(name: String, members: [String]) async throws -> String
    func startChat(did: String) async throws
    func deleteChat(_ chat: Chat) async
    func markRead(did: String) async
    func totalUnread() async -> Int
    func peerTicket(did: String) async -> String

    // MARK: social graph
    func follow(_ input: String) async throws
    func unfollow(did: String) async throws
    func following() async throws -> [Follow]
    func followers() async throws -> [Follow]
    func followBack(did: String) async throws
    func isFollowing(did: String) async -> Bool
    func userProfile(did: String) async throws -> UserProfile
    func drainNotifs() async -> [HeyNotification]

    // MARK: wallet
    func walletAddress() async -> String?
    func elastosDid() async -> String?
    func elaAddress() async -> String?
    func walletChains() async -> [ChainInfo]
    func walletBalance(chain: String) async -> WalletInfo?
    func balances(chain: String, includeHidden: Bool) async -> [TokenBal]
    func elaBalance() async -> String?
    func checkAddress(_ addr: String) async throws -> String      // returns checksummed
    func txStatus(chain: String, hash: String) async -> String
    func authorizeEvmSend(chain: String, to: String, amount: String) async -> SpendGrant
    func authorizeTokenSend(chain: String, contract: String, to: String, amount: String, decimals: Int) async -> SpendGrant
    func authorizeElaSend(to: String, amount: String) async -> SpendGrant
    func walletSend(chain: String, to: String, amount: String, auth: SpendGrant) async throws -> String
    func tokenSend(chain: String, contract: String, to: String, amount: String, decimals: Int, auth: SpendGrant) async throws -> String
    func elaSend(to: String, amount: String, auth: SpendGrant) async throws -> String
    func txHistory() async -> [TxRecord]
    func recordTx(_ tx: TxRecord) async
    func auditLog(limit: Int) async -> String

    // MARK: BEAM (optional — may be absent from the build)
    var beamAvailable: Bool { get }
    func beamAddress() async -> String?
    func beamBalance() async -> BeamBalance?
    func beamScan() async -> BeamScanResult
    func beamSend(token: String, amount: String, asset: Int) async throws -> BeamSendResult

    // MARK: tipping
    func resolveTip(did: String) async -> [String: String]
    func refreshContact(did: String) async -> [String: String]
    func publishTipAddresses() async -> Bool
    func notifyTip(to: String, symbol: String, amount: String, txHash: String) async

    // MARK: verse lane (sealed + ratcheted; in-memory inbox)
    func verseSend(did: String, payloadJSON: String) async throws -> Bool
    func versePoll() async -> String

    // MARK: 1:1 voice calls
    func callSend(did: String, payloadJSON: String) async -> Bool
    func callPoll() async -> [CallSignal]
    func voiceStart(peerTicket: String, isCaller: Bool) async
    func voicePeers() async -> Int
    func voiceSend(_ pcm: Data) async
    func voiceRecv(maxBytes: Int) async -> Data
    func voiceSetMuted(_ muted: Bool) async
    func voiceStop() async

    // MARK: content + network
    func content(cid: String) async -> Data?
    func netChanged()
}

// Convenience overloads so call-sites read cleanly.
extension HeyEngine {
    func feed() async throws -> [Post] { try await feed(limit: 50) }
    func profile() async throws -> Profile { try await profile(did: "") }
    func saveProfile(nickname: String, bio: String) async throws {
        try await saveProfile(nickname: nickname, bio: bio, avatarCid: "")
    }
    func balances(chain: String) async -> [TokenBal] { await balances(chain: chain, includeHidden: false) }
    func addComment(postId: String, text: String) async throws -> Comment {
        try await addComment(postId: postId, text: text, parent: "")
    }
    func send(_ chat: Chat, attachment data: Data, mime: String, name: String) async throws {
        try await sendAttachment(chat, data: data, mime: mime, name: name, text: "")
    }
}

enum HeyEngineFactory {
    /// The app-wide engine. MockEngine until the Rust xcframework is linked + RUST_ENGINE set.
    static let live: HeyEngine = {
        #if RUST_ENGINE
        return RustEngine()
        #else
        return MockEngine()
        #endif
    }()
}

enum AppPaths {
    static let appGroup = "group.os.elastos.hey.shared"
    /// The App Group container so the app and the Notification Service Extension
    /// share the same encrypted vault. Falls back to Application Support if the
    /// entitlement is missing (e.g. a bare simulator run).
    static var sharedContainer: URL {
        FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: appGroup)
            ?? FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
    }
    static var heyDir: URL { sharedContainer.appendingPathComponent("hey", isDirectory: true) }
}
