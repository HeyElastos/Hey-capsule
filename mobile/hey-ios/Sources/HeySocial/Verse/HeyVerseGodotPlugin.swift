import Foundation

// The Swift Godot-iOS plugin — the iOS equivalent of HeyVersePlugin.kt's
// @UsedByGodot surface. GDScript in mobile/hey-verse calls these; they delegate to
// VerseLane.shared (the session state machine + engine verse lane).
//
// Godot iOS plugins are registered via a .gdip config + a class exposed to the
// engine. With SwiftGodot you'd annotate these with @Callable on a @Godot class;
// with the classic ObjC plugin API you expose @objc methods and a method list.
// The bridge LOGIC below is engine-agnostic; only the registration glue differs and
// is wired on the Mac side where the Godot SDK is present. (See HEY_VERSE_IOS.md.)
//
// GDScript usage mirrors Android exactly, e.g.:
//   var did  = HeyVerse.localDid()
//   HeyVerse.invite(contact_did)
//   HeyVerse.sendMove(x, z, yaw, moving)
//   var snap = JSON.parse_string(HeyVerse.pollJson())

@objc final class HeyVerseGodotPlugin: NSObject {
    @objc static let shared = HeyVerseGodotPlugin()

    private var lane: VerseLane { VerseLane.shared }

    @objc func localDid() -> String { lane.localDid() }
    @objc func localName() -> String { lane.localName() }

    /// Your real Hey 1:1 contacts as [{did,name}] — the in-world invite picker.
    @objc func contactsJson() -> String {
        // Synchronous bridge for GDScript; contacts are cached by the app.
        VerseContactsCache.shared.json
    }

    @objc func invite(_ did: String) { lane.invite(did) }
    @objc func sendMove(_ x: Float, _ z: Float, _ yaw: Float, _ moving: Bool) { lane.sendMove(x: x, z: z, yaw: yaw, moving: moving) }
    @objc func sendChat(_ text: String) { lane.sendChat(text) }
    @objc func pollJson() -> String { lane.pollJSON() }

    /// game → app: open a popup sheet ("sash_faq", …) and "gameReady" overlay.
    @objc private(set) var gameReady = false
    @objc func gameReadyDidLoad() { gameReady = true }
    @objc var sheetRequest: String?
    @objc func openSheet(_ name: String) { sheetRequest = name }
}

/// Contacts must be available synchronously to GDScript; the app refreshes this
/// cache off the async engine when the verse opens.
final class VerseContactsCache {
    static let shared = VerseContactsCache()
    private(set) var json = "[]"
    func refresh(engine: HeyEngine) {
        Task {
            if let cs = try? await engine.contacts() {
                let arr = cs.map { ["did": $0.did, "name": $0.name] }
                json = (try? JSONSerialization.data(withJSONObject: arr)).flatMap { String(data: $0, encoding: .utf8) } ?? "[]"
            }
        }
    }
}
