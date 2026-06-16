import Foundation

// Swift mirrors of the engine's JSON shapes. The iOS app reuses the SAME engine as
// Android (capsules/hey-mobile-runtime), so these decode the EXACT JSON that the
// Android app parses in HeyApi.kt — snake_case keys (`author_name`, `sender_name`,
// …) are mapped with CodingKeys. RustEngine decodes FFI JSON straight into these;
// MockEngine constructs them directly. Source of truth: HeyApi.kt data classes.

/// The like reaction emoji (HeyApi.LIKE). A constant because reactions are keyed by emoji.
let HEY_LIKE = "\u{2764}\u{fe0f}" // ❤️

// MARK: - Feed

/// A media tile inside a post. `type` = "photo" | "video".
struct Media: Codable, Hashable, Identifiable {
    var cid: String
    var mime: String = ""
    var type: String = "photo"
    var name: String = ""
    var id: String { cid }
    var isVideo: Bool { type == "video" || mime.hasPrefix("video/") }
}

struct Post: Codable, Identifiable, Hashable {
    var id: String
    var author: String                 // author did
    var authorName: String = ""
    var authorAvatar: String = ""      // avatar cid
    var caption: String = ""
    var ts: Int64 = 0
    var media: [Media] = []

    enum CodingKeys: String, CodingKey {
        case id, author, caption, ts, media
        case authorName = "author_name"
        case authorAvatar = "author_avatar"
    }
}

/// Aggregate like/reaction state for a post (hey_get_reactions / hey_react). The
/// engine returns `{counts:{emoji:n}, mine:"<emoji>", total:n}`.
struct Reactions: Codable, Hashable {
    var counts: [String: Int] = [:]
    var mine: String = ""
    var total: Int = 0
    var likeCount: Int { counts[HEY_LIKE] ?? 0 }
    var liked: Bool { mine == HEY_LIKE }
    static let empty = Reactions()

    enum CodingKeys: String, CodingKey { case counts, mine, total }

    init() {}
    init(counts: [String: Int] = [:], mine: String = "", total: Int = 0) {
        self.counts = counts; self.mine = mine; self.total = total
    }

    // The engine emits `"mine": null` when the viewer hasn't reacted — decode that
    // (and a missing key) as "" instead of throwing on a non-optional String.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        counts = (try? c.decode([String: Int].self, forKey: .counts)) ?? [:]
        mine   = (try? c.decode(String.self, forKey: .mine)) ?? ""
        total  = (try? c.decode(Int.self, forKey: .total)) ?? 0
    }
}

struct Comment: Codable, Identifiable, Hashable {
    var id: String
    var author: String                 // author did
    var authorName: String = ""
    var text: String = ""
    var ts: Int64 = 0
    var parent: String = ""            // parent comment id ("" = top-level)

    enum CodingKeys: String, CodingKey {
        case id, author, text, ts, parent
        case authorName = "author_name"
    }
}

// MARK: - Identity / profile

struct Profile: Codable, Identifiable, Hashable {
    var did: String
    var nickname: String = ""
    var bio: String = ""
    var avatar: String = ""            // avatar cid
    var id: String { did }
    var displayName: String { nickname.isEmpty ? Self.short(did) : nickname }
    static func short(_ did: String) -> String { String(did.replacingOccurrences(of: "did:key:z", with: "").prefix(10)) + "…" }
}

/// A user's public profile as shown on UserProfileScreen — aggregated by the engine
/// from get_profile + post count + follow state (not a single FFI shape).
struct UserProfile: Codable, Hashable {
    var did: String
    var nickname: String = ""
    var bio: String = ""
    var avatar: String = ""
    var followers: Int = 0
    var following: Int = 0
    var posts: Int = 0
    var isFollowing: Bool = false
}

struct Follow: Codable, Identifiable, Hashable {
    var did: String
    var ticket: String = ""
    var id: String { did }
}

// MARK: - Chat

/// A row in the chat list — the engine merges contacts (hey_contacts) and groups
/// (hey_groups) into this shape (HeyApi.chats()). `id` = contact did OR group id.
struct Chat: Codable, Identifiable, Hashable {
    var id: String
    var name: String
    var preview: String = ""
    var ts: Int64 = 0
    var unread: Int = 0
    var isGroup: Bool = false
    var avatar: String = ""            // avatar cid (1:1 only)
    var online: Bool = false
}

struct Attachment: Codable, Hashable {
    var name: String
    var mime: String = ""
    var size: Int64 = 0
    /// The raw attachment JSON object (as a string) handed back to fetchAttachment
    /// so the engine resolves the same blob. Set by RustEngine; "" in mock.
    var raw: String = ""
    var isImage: Bool { mime.hasPrefix("image/") }
    var isVideo: Bool { mime.hasPrefix("video/") }

    enum CodingKeys: String, CodingKey { case name, mime, size }
}

struct Message: Codable, Identifiable, Hashable {
    var id: String
    var text: String = ""
    var ts: Int64 = 0
    var mine: Bool = false
    var sender: String = ""            // sender_name (group display label)
    var attachments: [Attachment] = []

    enum CodingKeys: String, CodingKey {
        case id, text, ts, mine, attachments
        case sender = "sender_name"
    }
}

struct MsgReaction: Codable, Hashable {
    var messageId: String
    var emoji: String
    var sender: String

    enum CodingKeys: String, CodingKey {
        case emoji
        case messageId = "message_id"
        case sender = "sender_did"
    }
}

// MARK: - Notifications

/// An Activity-tab notification (hey_drain_notifs). `kind` = like|comment|follow|tip|mention|group.
struct HeyNotification: Codable, Identifiable, Hashable {
    var kind: String
    var did: String = ""               // actor did
    var name: String = ""
    var text: String = ""
    var ts: Int64 = 0
    var postId: String = ""

    var id: String { "\(kind)|\(did)|\(ts)|\(postId)" }

    enum CodingKeys: String, CodingKey {
        case kind, did, name, text, ts
        case postId = "post_id"
    }
}

// MARK: - Calls

/// One inbound 1:1 call-control signal. `type` = offer | accept | decline | end.
struct CallSignal: Codable, Hashable {
    var from: String
    var type: String
    var callId: String = ""
    /// The raw signal payload JSON (verse/voice ticket etc.), opaque to the UI.
    var payloadJSON: String = ""
}

// MARK: - Wallet

struct ChainInfo: Codable, Identifiable, Hashable {
    var key: String                    // "esc" | "ethereum" | …
    var name: String
    var chainId: Int = 0
    var symbol: String = ""
    var id: String { key }
}

struct WalletInfo: Codable, Hashable {
    var address: String = ""
    var balance: String = "0"          // decimal native balance
    var wei: String = "0"
    var symbol: String = ""
}

struct TokenBal: Codable, Hashable, Identifiable {
    var symbol: String
    var name: String = ""
    var contract: String = ""
    var decimals: Int = 18
    var native: Bool = false
    var balance: String = "0"          // decimal
    var raw: String = "0"              // smallest-units string
    var id: String { native ? "native:\(symbol)" : contract }
}

struct TxRecord: Codable, Identifiable, Hashable {
    var chain: String
    var symbol: String
    var to: String
    var amount: String
    var hash: String
    var kind: String = "sent"          // sent | received | tip
    var ts: Int64 = 0
    var id: String { hash + String(ts) }
}

struct BeamBalance: Codable, Hashable {
    var beam: String = "0"
    var beamMaturing: String = "0"
    var beamx: String = "0"
}

struct BeamSendResult: Codable, Hashable {
    var txid: String
    var status: String
}

/// Outcome of a BEAM node sync (synced / still-syncing / error) so the wallet sheet
/// can show progress instead of a generic failure.
struct BeamScanResult: Codable, Hashable {
    var ok: Bool = false
    var synced: Bool = false
    var height: Int64 = 0
    var error: String? = nil
}

/// A spend grant minted by guard.rs (one-shot, 90s TTL, bound to kind+to+amount).
typealias SpendGrant = String

// MARK: - Carrier health

/// Connection snapshot for the badge/sheets. The engine (hey_carrier_health) emits
/// `{online, direct, peer_count, relay_peers, direct_peers, …}` — NOT `peers`/`mode`
/// — so we decode `peer_count`→`peers` and derive `mode` from the `direct` bool,
/// matching how the Android ConnectionSheet reads the same JSON.
struct CarrierHealth: Codable, Hashable {
    var online: Bool = false
    var peers: Int = 0
    var relay: String = ""             // engine doesn't surface a relay URL → stays ""
    var direct: Bool = false
    /// "direct" | "relay" — derived (the engine carries `direct`, not a `mode` string).
    var mode: String { direct ? "direct" : "relay" }

    enum CodingKeys: String, CodingKey {
        case online, relay, direct
        case peers = "peer_count"
    }

    init() {}

    /// Convenience init for MockEngine (and any direct construction). `mode` is derived
    /// from `direct`, so pass `direct:` rather than a "direct"/"relay" string.
    init(online: Bool = false, peers: Int = 0, relay: String = "", direct: Bool = false) {
        self.online = online; self.peers = peers; self.relay = relay; self.direct = direct
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        online = (try? c.decode(Bool.self, forKey: .online)) ?? false
        peers  = (try? c.decode(Int.self,  forKey: .peers))  ?? 0
        relay  = (try? c.decode(String.self, forKey: .relay)) ?? ""
        direct = (try? c.decode(Bool.self, forKey: .direct)) ?? false
    }
}
