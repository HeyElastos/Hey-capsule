import Foundation
import LocalAuthentication

// OPTIONAL app lock — off by default. The iOS counterpart of Android's AppLock.
// When enabled, Hey asks for Face ID / Touch ID (verified in the Secure Enclave) or
// the device passcode before opening, revealing the recovery phrase, or authorizing a
// spend. This is a layer ON TOP of two things that are always true on iOS:
//   • iOS sandboxes Hey's container so other apps can't read it.
//   • iOS encrypts app data at rest (Data Protection), keyed to the device passcode.
// So the lock guards against someone holding your unlocked phone; the data itself is
// already isolated and encrypted by the OS (and additionally by SecureEnclaveVault).
enum AppLock {
    private static let key = "app_lock_enabled"
    private static var defaults: UserDefaults {
        UserDefaults(suiteName: AppPaths.appGroup) ?? .standard
    }

    /// Can this device do a biometric OR passcode check? (= Android's
    /// BIOMETRIC_STRONG | DEVICE_CREDENTIAL.)
    static func available() -> Bool {
        var err: NSError?
        return LAContext().canEvaluatePolicy(.deviceOwnerAuthentication, error: &err)
    }

    /// faceID / touchID / opticID / none — for labelling the unlock button.
    static func biometryType() -> LABiometryType {
        let ctx = LAContext()
        _ = ctx.canEvaluatePolicy(.deviceOwnerAuthentication, error: nil)
        return ctx.biometryType
    }

    static var enabled: Bool {
        get { defaults.bool(forKey: key) }
        set { defaults.set(newValue, forKey: key) }
    }

    /// Prompt for unlock. Returns true on success. `.deviceOwnerAuthentication` allows
    /// biometric OR the device passcode fallback (matches Android's allowed authenticators).
    static func prompt(reason: String = "Verify it's you to open Hey") async -> Bool {
        let ctx = LAContext()
        ctx.localizedFallbackTitle = "Use Passcode"
        guard ctx.canEvaluatePolicy(.deviceOwnerAuthentication, error: nil) else { return false }
        return await withCheckedContinuation { cont in
            ctx.evaluatePolicy(.deviceOwnerAuthentication, localizedReason: reason) { ok, _ in
                cont.resume(returning: ok)
            }
        }
    }
}
