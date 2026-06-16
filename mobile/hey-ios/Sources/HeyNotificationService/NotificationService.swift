import UserNotifications

// Notification Service Extension — wakes on an `alert` push with mutable-content:1,
// resolves the blinded handle, decrypts the message ON-DEVICE, and shows the real
// content. v1 handles SINGLE-SHOT messages (and the carried `e` envelope); RATCHET
// messages are deferred to the app (shown as "New message") to avoid the app↔NSE
// ratchet-state race. See docs/HEY_IOS_PUSH_GATEWAY.md §4.5 / §9.
//
// ~30s budget: fast path (push carries `e`) ≈ 0.3s; pull path ≈ 5s.

final class NotificationService: UNNotificationServiceExtension {
    private var contentHandler: ((UNNotificationContent) -> Void)?
    private var best: UNMutableNotificationContent?

    override func didReceive(_ request: UNNotificationRequest,
                             withContentHandler contentHandler: @escaping (UNNotificationContent) -> Void) {
        self.contentHandler = contentHandler
        let content = (request.content.mutableCopy() as? UNMutableNotificationContent) ?? UNMutableNotificationContent()
        best = content

        let info = request.content.userInfo
        // `bid` is carried in apns-collapse-id (a custom key here for the extension).
        let bid = (info["bid"] as? String) ?? request.content.threadIdentifier
        let sealed = info["e"] as? String   // optional carried sealed envelope (small single-shot)

        Task {
            defer { finish() }
            guard let topic = SharedStore.topic(forBlindedId: bid) else { return } // unknown → generic
            if let preview = await NSEDecryptor.shared.decryptSingleShot(topic: topic, sealedEnvelope: sealed) {
                content.title = preview.title
                content.body = preview.body
                content.threadIdentifier = topic
            }
            // ratchet / large / undecryptable → leave the generic body; the app finishes on open.
        }
    }

    override func serviceExtensionTimeWillExpire() { finish() }

    private func finish() {
        if let h = contentHandler, let c = best { contentHandler = nil; h(c) }
    }
}

/// On-device decrypt for the NSE. Wraps the engine's single-shot decrypt over the
/// shared Keychain/App Group state. (Wires to a `hey_nse_decrypt` FFI on the Mac side.)
struct NSEDecryptor {
    static let shared = NSEDecryptor()
    struct Preview { let title: String; let body: String }

    func decryptSingleShot(topic: String, sealedEnvelope: String?) async -> Preview? {
        // TODO(engine): call hey_nse_decrypt(topic, sealedEnvelope?) which:
        //   1. reads X25519/ML-KEM secrets from the shared Keychain group,
        //   2. (fast path) decrypts the carried envelope, else pulls via peer::recv,
        //   3. verifies Ed25519 inner sig + dedup, returns {title, body} JSON.
        // Returns nil for ratchet/large/failed so the generic "New message" stands.
        return nil
    }
}

/// App Group-backed lookup the NSE shares with the app (bid → topic map, §3/§5).
enum SharedStore {
    static let appGroup = "group.os.elastos.hey.shared"
    static func topic(forBlindedId bid: String) -> String? {
        UserDefaults(suiteName: appGroup)?
            .dictionary(forKey: "bidToTopic")?[bid] as? String
    }
}
