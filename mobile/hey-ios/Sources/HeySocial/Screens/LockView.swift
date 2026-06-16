import SwiftUI
import LocalAuthentication

// Locked state — the iOS port of Android's LockScreen (MainActivity.kt:5499-5521).
// Shown when the vault is ON: the seed is sealed in the Secure Enclave and full
// decryption resumes only after a Face ID / Touch ID / passcode check.
//
// The unlock button label adapts to the device biometry (AppLock.biometryType()),
// e.g. "Unlock with Face ID". On success it tries IdentityVault.unseal() (which itself
// triggers the biometric prompt via the key's ACL) and hands the recovered seed up
// through onUnlock; if there's no sealed blob it falls back to a plain AppLock.prompt.
//
// "Restore from recovery phrase" is always available: if the device lock changed/was
// removed the hardware key is permanently invalidated and biometric unlock can never
// succeed — a user who still has their phrase must never be bricked.
struct LockView: View {
    @Environment(\.colorScheme) private var scheme
    /// Called with the recovered seed when the vault unseals, or "" when the unlock was a
    /// plain presence check (no sealed blob to recover — already-derived identity).
    let onUnlock: (String) -> Void
    let onRestore: () -> Void

    @State private var working = false
    @State private var failed = false

    private var biometry: LABiometryType { AppLock.biometryType() }
    private var biometryName: String {
        switch biometry {
        case .faceID: return "Face ID"
        case .touchID: return "Touch ID"
        default: return "passcode"   // .opticID / .none → passcode wording
        }
    }
    private var biometryIcon: String {
        switch biometry {
        case .faceID: return "faceid"
        case .touchID: return "touchid"
        default: return "lock.fill"
        }
    }
    private var unlockLabel: String {
        biometry == .none ? "Unlock" : "Unlock with \(biometryName)"
    }

    var body: some View {
        ZStack {
            FrostBackground()
            VStack(spacing: 0) {
                Spacer()
                ZStack {
                    Circle().fill(Hey.avatarGradient).frame(width: 96, height: 96)
                    Image(systemName: "lock.fill").font(.system(size: 46)).foregroundStyle(Hey.navy)
                }
                Spacer().frame(height: 20)
                Text("Hey is locked").font(HeyFont.subtitle.weight(.bold)).foregroundStyle(Hey.ink(scheme))
                Spacer().frame(height: 6)
                Text("Verify it's you to open your data.")
                    .font(HeyFont.callout).foregroundStyle(Hey.muted(scheme))
                    .multilineTextAlignment(.center)
                if failed {
                    Spacer().frame(height: 8)
                    Text("Couldn't verify it's you. Try again.")
                        .font(HeyFont.caption).foregroundStyle(Hey.like)
                }
                Spacer().frame(height: 24)

                Button(action: unlock) {
                    HStack(spacing: 8) {
                        if working {
                            ProgressView().tint(Hey.navy)
                        } else {
                            Image(systemName: biometryIcon).font(.system(size: 18))
                            Text(unlockLabel).font(.system(size: 16, weight: .bold))
                        }
                    }
                    .foregroundStyle(Hey.navy)
                    .padding(.vertical, 14).padding(.horizontal, 24)
                    .background(Hey.gold, in: Capsule())
                }
                .disabled(working)

                Spacer().frame(height: 14)
                Button(action: onRestore) {
                    Text("Restore from recovery phrase")
                        .font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))
                }
                Spacer()
            }
            .padding(32)
        }
        // Android auto-triggers the prompt on first composition (LaunchedEffect → onUnlock).
        .task { if !working { unlock() } }
    }

    private func unlock() {
        guard !working else { return }
        working = true
        failed = false
        Task {
            // Prefer the real vault: unseal recovers the seed AND drives the biometric
            // prompt via the key ACL. Fall back to a plain presence check otherwise.
            var seed: String? = nil
            if IdentityVault.hasSealed() {
                seed = IdentityVault.unseal(reason: "Unlock your Hey identity")
            } else if await AppLock.prompt(reason: "Verify it's you to open Hey") {
                seed = ""
            }
            await MainActor.run {
                working = false
                if let seed {
                    onUnlock(seed)
                } else {
                    failed = true
                }
            }
        }
    }
}
