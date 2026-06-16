import UIKit
import UserNotifications
import PushKit
import BackgroundTasks

// Background delivery wiring (see docs/HEY_IOS_PUSH_GATEWAY.md):
//   • APNs alert push + Notification Service Extension → text DMs while closed
//   • PushKit voip → incoming calls (must reportNewIncomingCall to CallKit)
//   • BGAppRefreshTask → opportunistic catch-up
// This delegate registers tokens; the gateway/outbox + NSE do the heavy lifting.

final class AppDelegate: NSObject, UIApplicationDelegate {
    static let refreshTaskID = "os.elastos.hey.social.refresh"

    func application(_ application: UIApplication,
                     didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        UNUserNotificationCenter.current().delegate = self
        UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .badge, .sound]) { granted, _ in
            guard granted else { return }
            DispatchQueue.main.async { application.registerForRemoteNotifications() }
        }
        registerPushKit()
        registerBackgroundRefresh()
        return true
    }

    // ── APNs (text) ──────────────────────────────────────────────────────────
    func application(_ application: UIApplication,
                     didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
        let hex = deviceToken.map { String(format: "%02x", $0) }.joined()
        // Hand the token to the engine, which registers BLINDED handles with the gateway
        // (bid = HMAC(device-salt, queue-topic)) — the gateway never learns the social graph.
        PushRegistrar.shared.register(apnsTokenHex: hex)
    }

    func application(_ application: UIApplication,
                     didFailToRegisterForRemoteNotificationsWithError error: Error) {
        print("APNs registration failed: \(error)")
    }

    // ── PushKit (voice calls) ────────────────────────────────────────────────
    private let voipRegistry = PKPushRegistry(queue: .main)
    private func registerPushKit() {
        voipRegistry.delegate = self
        voipRegistry.desiredPushTypes = [.voIP]
    }

    // ── BGAppRefreshTask (catch-up) ──────────────────────────────────────────
    private func registerBackgroundRefresh() {
        BGTaskScheduler.shared.register(forTaskWithIdentifier: Self.refreshTaskID, using: nil) { task in
            self.handleRefresh(task as! BGAppRefreshTask)
        }
        scheduleRefresh()
    }

    func scheduleRefresh() {
        let req = BGAppRefreshTaskRequest(identifier: Self.refreshTaskID)
        req.earliestBeginDate = Date(timeIntervalSinceNow: 15 * 60)
        try? BGTaskScheduler.shared.submit(req)
    }

    private func handleRefresh(_ task: BGAppRefreshTask) {
        scheduleRefresh()
        let work = Task {
            // Re-graft topics + drain queues for anything the push missed.
            await VerseLane.shared.drainOnce()
            task.setTaskCompleted(success: true)
        }
        task.expirationHandler = { work.cancel() }
    }
}

extension AppDelegate: UNUserNotificationCenterDelegate {
    func userNotificationCenter(_ center: UNUserNotificationCenter,
                                willPresent notification: UNNotification) async -> UNNotificationPresentationOptions {
        [.banner, .badge, .sound]
    }
}

extension AppDelegate: PKPushRegistryDelegate {
    func pushRegistry(_ registry: PKPushRegistry, didUpdate pushCredentials: PKPushCredentials, for type: PKPushType) {
        let hex = pushCredentials.token.map { String(format: "%02x", $0) }.joined()
        PushRegistrar.shared.registerVoip(tokenHex: hex)
    }

    func pushRegistry(_ registry: PKPushRegistry, didReceiveIncomingPushWith payload: PKPushPayload,
                      for type: PKPushType, completion: @escaping () -> Void) {
        // CallKit is MANDATORY here or iOS terminates the app. The caller name is
        // decrypted on-device from the sealed call-offer envelope (sealed-sender).
        CallController.shared.reportIncomingCall(payload: payload.dictionaryPayload, completion: completion)
    }
}

/// Thin shim so the delegate doesn't reach into the engine directly.
final class PushRegistrar {
    static let shared = PushRegistrar()
    private let engine = HeyEngineFactory.live
    func register(apnsTokenHex: String) {
        Task { /* RustEngine: hey_register_push_token(apnsTokenHex) — no-op on MockEngine */ }
    }
    func registerVoip(tokenHex: String) {
        Task { /* register the voip token with the gateway for call wakes */ }
    }
}
