import Foundation

// Swift port of HeyVersePlugin.kt — the Hey Verse <-> runtime bridge + session
// state machine. Verse traffic rides the engine's dedicated lane (verseSend /
// versePoll): sealed + ratcheted like a DM but diverted to an in-memory inbox.
//
// Session model is LIVE-ONLY: presence = receiving signals; a peer silent >12s
// (or "bye") drops, and a fresh invite is needed to rejoin.
//
// Protocol (payload JSON): k = inv | ok | mv | chat | bye
//   inv {name,w,ts}  ok {name,w}  mv {x,z,yw,m,w}  chat {tx}   // w = "home"|"city"

struct VerseInvite: Equatable {
    let did: String
    let name: String
    let world: String
}

final class VersePeer {
    var name: String
    var x: Float = 0; var z: Float = 2; var yaw: Float = 0
    var moving = false
    var zone = "home"
    var last = Date()
    init(name: String) { self.name = name }
}

/// App-wide singleton. `start()` runs the single drain; signals route to the live
/// session, invites surface app-wide via `onInvite` (mirrors startLane()).
final class VerseLane {
    static let shared = VerseLane()

    private var engine: HeyEngine?
    private var onInvite: ((VerseInvite) -> Void)?
    private var timer: Timer?
    private let lock = NSLock()

    // session state (HeyVersePlugin instance fields)
    private var peers: [String: VersePeer] = [:]
    private var chats: [(String, String)] = []     // inbound (did, text)
    private var ended: [String] = []
    private var uiQueue: [String] = []
    private var pendingInvite: VerseInvite?
    private var acceptedInvite: VerseInvite?
    private var joinWorld: String?

    private(set) var myDid = ""
    private(set) var myName = "me"
    private var myZone = "home"
    private var lastMove = Date(timeIntervalSince1970: 0)

    func attach(engine: HeyEngine, onInvite: @escaping (VerseInvite) -> Void) {
        self.engine = engine
        self.onInvite = onInvite
        Task {
            if let me = try? await engine.whoami() {
                myDid = me.did
                if !me.nickname.isEmpty { myName = me.nickname }
            }
        }
    }

    func start() {
        guard timer == nil else { return }
        timer = Timer.scheduledTimer(withTimeInterval: 0.4, repeats: true) { [weak self] _ in
            Task { await self?.drainOnce() }
        }
    }

    /// One pass over the verse inbox (startLane loop body). Also the BGAppRefresh hook.
    func drainOnce() async {
        guard let engine else { return }
        let json = await engine.versePoll()
        guard let arr = try? JSONSerialization.jsonObject(with: Data(json.utf8)) as? [[Any]] else { return }
        for pair in arr {
            guard let from = pair.first as? String, let p = pair.last as? [String: Any] else { continue }
            let k = p["k"] as? String ?? ""
            if k == "inv" {
                let ts = (p["ts"] as? NSNumber)?.int64Value ?? 0
                let fresh = ts == 0 || (Int64(Date().timeIntervalSince1970 * 1000) - ts) < 90_000
                if fresh {
                    let inv = VerseInvite(did: from,
                                          name: (p["name"] as? String).flatMap { $0.isEmpty ? nil : $0 } ?? Self.shortDid(from),
                                          world: p["w"] as? String ?? "home")
                    lock.lock(); pendingInvite = inv; lock.unlock()
                    onInvite?(inv)
                }
            } else {
                onSignal(from: from, p: p)
            }
        }
    }

    // ── invite popup ──────────────────────────────────────────────────────────
    func accept() { lock.lock(); acceptedInvite = pendingInvite; pendingInvite = nil; lock.unlock() }
    func decline() { lock.lock(); pendingInvite = nil; lock.unlock() }

    // ── inbound signals (HeyVersePlugin.onSignal) ──────────────────────────────
    private func onSignal(from: String, p: [String: Any]) {
        let now = Date()
        lock.lock(); defer { lock.unlock() }
        switch p["k"] as? String {
        case "ok":
            let peer = peers[from] ?? VersePeer(name: (p["name"] as? String).flatMap { $0.isEmpty ? nil : $0 } ?? Self.shortDid(from))
            peer.name = (p["name"] as? String).flatMap { $0.isEmpty ? nil : $0 } ?? peer.name
            peer.zone = p["w"] as? String ?? peer.zone
            peer.last = now
            peers[from] = peer
        case "mv":
            let peer = peers[from] ?? VersePeer(name: Self.shortDid(from))
            peer.x = (p["x"] as? NSNumber)?.floatValue ?? 0
            peer.z = (p["z"] as? NSNumber)?.floatValue ?? 2
            peer.yaw = (p["yw"] as? NSNumber)?.floatValue ?? 0
            peer.moving = (p["m"] as? NSNumber)?.boolValue ?? false
            peer.zone = p["w"] as? String ?? peer.zone
            peer.last = now
            peers[from] = peer
        case "chat":
            if let tx = p["tx"] as? String, !tx.isEmpty {
                peers[from]?.last = now
                chats.append((from, tx))
            }
        case "bye":
            if peers.removeValue(forKey: from) != nil { ended.append(from) }
        default: break
        }
    }

    // ── outbound (engine verse lane) ───────────────────────────────────────────
    private func send(_ did: String, _ payload: [String: Any]) {
        guard let engine, let data = try? JSONSerialization.data(withJSONObject: payload),
              let s = String(data: data, encoding: .utf8) else { return }
        Task { _ = try? await engine.verseSend(did: did, payloadJSON: s) }
    }

    func invite(_ did: String) {
        send(did, ["k": "inv", "name": myName, "w": myZone, "ts": Int64(Date().timeIntervalSince1970 * 1000)])
    }

    // ── surface for the Godot plugin (@UsedByGodot equivalents) ─────────────────
    func localDid() -> String { myDid }
    func localName() -> String { myName }

    func sendMove(x: Float, z: Float, yaw: Float, moving: Bool) {
        myZone = z < -100 ? "city" : "home"
        let now = Date()
        if now.timeIntervalSince(lastMove) < 0.2 { return }
        lastMove = now
        let payload: [String: Any] = ["k": "mv", "x": Double(x), "z": Double(z), "yw": Double(yaw), "m": moving, "w": myZone]
        for did in presentDids() { send(did, payload) }
    }

    func sendChat(_ text: String) {
        let payload: [String: Any] = ["k": "chat", "tx": text]
        for did in presentDids() { send(did, payload) }
    }

    private func presentDids() -> [String] { lock.lock(); defer { lock.unlock() }; return Array(peers.keys) }

    /// Drain for the game: {peers:{did:{x,z,yw,m,w,name}}, chats, ended, ui, me, join?}
    func pollJSON() -> String {
        // accepted invite: greet the inviter + tell the game which world
        lock.lock()
        if let inv = acceptedInvite {
            acceptedInvite = nil
            let peer = peers[inv.did] ?? VersePeer(name: inv.name)
            peer.zone = inv.world; peer.last = Date(); peers[inv.did] = peer
            joinWorld = inv.world
            lock.unlock()
            send(inv.did, ["k": "ok", "name": myName, "w": inv.world])
            lock.lock()
        }
        let now = Date()
        var po: [String: Any] = [:]
        for (did, peer) in peers {
            if now.timeIntervalSince(peer.last) > 12 { ended.append(did); peers.removeValue(forKey: did); continue }
            po[did] = ["x": Double(peer.x), "z": Double(peer.z), "yw": Double(peer.yaw),
                       "m": peer.moving, "w": peer.zone, "name": peer.name]
        }
        let out: [String: Any] = [
            "peers": po,
            "chats": chats.map { [$0.0, $0.1] },
            "ended": ended,
            "ui": uiQueue,
            "me": ["did": myDid, "name": myName],
            "join": joinWorld as Any,
        ].compactMapValues { $0 }
        chats.removeAll(); ended.removeAll(); uiQueue.removeAll(); joinWorld = nil
        lock.unlock()
        return (try? JSONSerialization.data(withJSONObject: out)).flatMap { String(data: $0, encoding: .utf8) } ?? "{}"
    }

    /// Compose → game command queue (postUi).
    func postUi(_ cmd: String) { lock.lock(); uiQueue.append(cmd); lock.unlock() }

    static func shortDid(_ did: String) -> String { did.count > 12 ? "\(did.prefix(8))…\(did.suffix(4))" : did }
}
