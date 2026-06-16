import SwiftUI

@main
struct HeyApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
    @StateObject private var store = AppStore()
    @Environment(\.scenePhase) private var scenePhase

    init() {
        // Register the in-process content provider URLProtocol so ContentImage /
        // hey-content://<cid> resolves media by namespace (never by network).
        URLProtocol.registerClass(ContentURLProtocol.self)
    }

    var body: some Scene {
        WindowGroup {
            RootView()
                .environmentObject(store)
                .task { await store.boot() }
                .onChange(of: scenePhase) { phase in
                    switch phase {
                    case .background: store.didEnterBackground()
                    case .active:     store.didBecomeActive()
                    default: break
                    }
                }
        }
    }
}

/// App-wide observable state: the boot/lock state machine, the live engine, and the
/// background polling that drives presence, unread, activity and the verse/call lanes.
@MainActor
final class AppStore: ObservableObject {
    let engine: HeyEngine = HeyEngineFactory.live

    /// The gate the UI renders. Mirrors Android HeyApp's unlocked/needChoice/welcomed/ready.
    enum Phase: Equatable {
        case starting                       // deciding what to show
        case locked                         // vault ON + sealed → biometric unseal first
        case welcome                        // first run: create / restore choice
        case generating(restoring: Bool)    // the DID-generation animation
        case ready                          // the main app
    }

    @Published var phase: Phase = .starting
    @Published var me: Profile?
    @Published var online = false
    @Published var peers = 0
    @Published var unread = 0
    @Published var activityCount = 0        // followers/activity total (Android notifCount)
    @Published var feedRev: Int64 = 0
    @Published var verseInvite: VerseInvite?
    @Published var incomingCall: CallSignal?

    private var engineStarted = false
    private var polling = false
    private var backgroundedAt: Date?

    private var defaults: UserDefaults { UserDefaults(suiteName: AppPaths.appGroup) ?? .standard }
    private var welcomed: Bool {
        get { defaults.bool(forKey: "welcomed") }
        set { defaults.set(newValue, forKey: "welcomed") }
    }
    /// Activity badge = new since last opened the bell.
    var activitySeen: Int {
        get { defaults.integer(forKey: "activity_seen") }
        set { defaults.set(newValue, forKey: "activity_seen") }
    }
    var activityBadge: Int { max(0, activityCount - activitySeen) }

    // MARK: - boot / gate

    func boot() async {
        guard phase == .starting else { return }
        // Vault ON + sealed → gate on a biometric unseal before the engine starts.
        if IdentityVault.isOn && IdentityVault.hasSealed() && AppLock.available() {
            phase = .locked
            return
        }
        // First run → let the user choose create vs restore BEFORE the engine mints an identity.
        if !welcomed {
            phase = .welcome
            return
        }
        await startEngine(restorePhrase: nil)
        phase = .ready
        startPolling()
    }

    /// Welcome → "Create new identity": boot fresh, then play the generation animation.
    func createIdentity() async {
        await startEngine(restorePhrase: nil)
        phase = .generating(restoring: false)
    }

    /// Welcome → restore phrase (or a lock-screen restore): boot deriving from the phrase.
    func restoreIdentity(phrase: String) async {
        await startEngine(restorePhrase: phrase)
        phase = .generating(restoring: true)
    }

    /// The generation animation finished → enter the app (and remember we onboarded).
    func finishOnboarding() {
        welcomed = true
        phase = .ready
        startPolling()
    }

    /// LockView unlocked: `seed` = the recovered phrase (vault unseal) or "" (presence-only).
    func unlock(seed: String) async {
        await startEngine(restorePhrase: seed.isEmpty ? nil : seed)
        welcomed = true
        phase = .ready
        startPolling()
    }

    /// Biometric unlock can't recover the seed (device lock changed / wrong account) →
    /// clear the dead seal and route to create/restore (the phrase still recovers everything).
    func failUnlockToRestore() {
        IdentityVault.clear()
        phase = .welcome
    }

    private func startEngine(restorePhrase: String?) async {
        guard !engineStarted else { return }   // re-lock keeps the runtime up (Android Option A)
        do {
            if let p = restorePhrase, !p.isEmpty {
                try await engine.restore(phrase: p)
            } else {
                try await engine.start()
            }
            engineStarted = true
            me = try? await engine.whoami()
            VerseLane.shared.attach(engine: engine) { [weak self] invite in
                Task { @MainActor in self?.verseInvite = invite }
            }
            VerseLane.shared.start()
        } catch {
            print("AppStore.startEngine failed: \(error)")
        }
    }

    func refreshMe() async { me = try? await engine.whoami() }

    // MARK: - background re-lock (vault only)

    func didEnterBackground() {
        backgroundedAt = Date()
    }

    func didBecomeActive() {
        guard IdentityVault.isOn, phase == .ready, let at = backgroundedAt else { return }
        // Long background → re-gate behind a fresh biometric (the seed stays in memory).
        if Date().timeIntervalSince(at) > 120 { phase = .locked }
        backgroundedAt = nil
    }

    // MARK: - polling (Android HeyApp LaunchedEffect loops)

    private func startPolling() {
        guard !polling else { return }
        polling = true

        // presence + unread + activity (~3s)
        Task { [weak self] in
            while let self, await self.polling {
                let health = await self.engine.carrierHealth()
                let un = await self.engine.totalUnread()
                let followers = (try? await self.engine.followers().count) ?? 0
                await MainActor.run {
                    self.online = health.online; self.peers = health.peers
                    self.unread = un; self.activityCount = followers
                }
                try? await Task.sleep(nanoseconds: 3_000_000_000)
            }
        }
        // feed auto-refresh signal (~1.5s)
        Task { [weak self] in
            while let self, await self.polling {
                let r = await self.engine.feedRev()
                await MainActor.run { if r != self.feedRev { self.feedRev = r } }
                try? await Task.sleep(nanoseconds: 1_500_000_000)
            }
        }
        // 1:1 voice-call signals (~1s)
        Task { [weak self] in
            while let self, await self.polling {
                let signals = await self.engine.callPoll()
                if let offer = signals.first(where: { $0.type == "offer" }) {
                    await MainActor.run { self.incomingCall = offer }
                } else if signals.contains(where: { $0.type == "end" || $0.type == "decline" }) {
                    await MainActor.run { self.incomingCall = nil }
                }
                try? await Task.sleep(nanoseconds: 1_000_000_000)
            }
        }
    }
}
