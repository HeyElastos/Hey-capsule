import SwiftUI
import LocalAuthentication

// Port of Android SettingsSheet (MainActivity.kt:2172-2334).
//
// This is the iOS SECURITY surface. The Android sheet seals the seed in
// StrongBox / Knox Vault / TEE; on iOS that hardware is the SECURE ENCLAVE, so
// the copy is mapped accordingly. Wires to:
//   • AppLock        — optional biometric/passcode gate (App Lock switch).
//   • IdentityVault  — seals the seed under a biometry-gated Secure Enclave key.
//   • engine.recoveryPhrase() — the 12-word BIP39 phrase, revealed behind a fresh
//                               AppLock.prompt, exactly like Android's requireAuth.
//
// Presented as a `.sheet`; the orchestrator owns cross-group navigation, so we take
// onClose / onShowConnection / onShowAbout closures and present nothing outside this group.
struct SettingsSheet: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var scheme
    @Environment(\.dismiss) private var dismiss

    let did: String
    var onClose: () -> Void = {}
    var onShowQr: () -> Void = {}
    var onShowConnection: () -> Void = {}
    var onShowAbout: () -> Void = {}

    // App Lock (biometric/passcode gate, off by default).
    @State private var lockOn = AppLock.enabled
    // Identity Vault (seed sealed in the Secure Enclave). Single-mode: once sealed, stays sealed.
    @State private var vaultOn = IdentityVault.isOn
    @State private var vaultBusy = false
    // Recovery-phrase reveal (only after a fresh AppLock prompt).
    @State private var phrase: String?
    @State private var toast: String?

    private var vaultable: Bool { IdentityVault.available() }
    private var sealed: Bool { IdentityVault.hasSealed() }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    identityCard
                    connectionRow
                    appLockCard
                    vaultCard
                    appearanceRow
                }
                .padding(20)
            }
            .scrollContentBackground(.hidden)
            .background(FrostBackground().ignoresSafeArea())
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { onClose(); dismiss() }.tint(Hey.goldInk(scheme))
                }
            }
        }
        .tint(Hey.goldInk(scheme))
        // Recovery-phrase reveal — mirrors Android's AlertDialog.
        .sheet(item: Binding(get: { phrase.map { PhraseBox(text: $0) } }, set: { phrase = $0?.text })) { box in
            RecoveryPhraseView(phrase: box.text)
        }
        .overlay(alignment: .bottom) {
            if let toast {
                Text(toast)
                    .font(HeyFont.caption).foregroundStyle(Hey.ink(scheme))
                    .padding(.horizontal, 14).padding(.vertical, 10)
                    .glass(14).padding(.bottom, 28)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
            }
        }
        .animation(.spring(response: 0.4, dampingFraction: 0.85), value: toast)
    }

    // ── Your identity (DID + copy) ──
    private var identityCard: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label { Text("Your identity").font(HeyFont.author) } icon: {
                Image(systemName: "person.text.rectangle.fill").foregroundStyle(Hey.goldInk(scheme))
            }
            .foregroundStyle(Hey.ink(scheme))
            Text(did).font(HeyFont.mono(12)).foregroundStyle(Hey.muted(scheme))
                .textSelection(.enabled)
            HStack(spacing: 10) {
                Button {
                    UIPasteboard.general.string = did
                    flash("DID copied")
                } label: {
                    Label("Copy DID", systemImage: "doc.on.doc").font(HeyFont.label)
                }
                .buttonStyle(.bordered).tint(Hey.goldInk(scheme))

                Button { onShowQr() } label: {
                    Label("My QR", systemImage: "qrcode").font(HeyFont.label)
                }
                .buttonStyle(.bordered).tint(Hey.goldInk(scheme))
            }
            Text("This DID is your sovereign identity — it signs everything you create. To connect with someone, share your invite link or QR; a DID alone can't open a private channel.")
                .font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14).glass()
    }

    // ── How Hey connects (→ ConnectionSheet) ──
    private var connectionRow: some View {
        Button {
            onShowConnection()
        } label: {
            HStack(spacing: 8) {
                Image(systemName: "point.3.connected.trianglepath.dotted").foregroundStyle(Hey.goldInk(scheme))
                Text("How Hey connects").font(HeyFont.author).foregroundStyle(Hey.ink(scheme))
                Spacer()
                Image(systemName: "chevron.right").font(.system(size: 13)).foregroundStyle(Hey.muted(scheme))
            }
            .padding(14).glass()
        }
        .buttonStyle(.plain)
    }

    // ── App Lock (biometric / passcode gate) ──
    private var appLockCard: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .top, spacing: 8) {
                Image(systemName: lockOn ? "lock.fill" : "lock.open.fill")
                    .foregroundStyle(lockOn ? Hey.good(scheme) : Hey.goldInk(scheme))
                VStack(alignment: .leading, spacing: 2) {
                    Text("App Lock").font(HeyFont.author).foregroundStyle(Hey.ink(scheme))
                    Text(AppLock.available()
                         ? "Require \(biometryName) or your passcode to open Hey"
                         : "No biometric or passcode set up on this device")
                        .font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))
                }
                Spacer()
                Toggle("", isOn: lockBinding).labelsHidden()
                    .tint(Hey.gold).disabled(!AppLock.available())
            }
        }
        .padding(14).glass()
    }

    private var lockBinding: Binding<Bool> {
        Binding(
            get: { lockOn },
            set: { want in
                Task {
                    if want {
                        // Confirm the user can pass the gate before turning it on.
                        let ok = await AppLock.prompt(reason: "Turn on App Lock")
                        if ok { AppLock.enabled = true; lockOn = true; flash("App Lock on") }
                        else { lockOn = false }
                    } else {
                        let ok = await AppLock.prompt(reason: "Turn off App Lock")
                        if ok { AppLock.enabled = false; lockOn = false; flash("App Lock off") }
                        else { lockOn = true }
                    }
                }
            }
        )
    }

    // ── Identity Vault (seal seed in the Secure Enclave) ──
    private var vaultCard: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .top, spacing: 8) {
                Image(systemName: vaultOn ? "lock.shield.fill" : "lock.shield")
                    .foregroundStyle(vaultOn ? Hey.good(scheme) : Hey.goldInk(scheme))
                VStack(alignment: .leading, spacing: 2) {
                    Text("Identity Vault").font(HeyFont.author).foregroundStyle(Hey.ink(scheme))
                    Text(vaultable
                         ? "Encrypt your keys in the Secure Enclave; unlock with \(biometryName)"
                         : "No biometric set up on this device")
                        .font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))
                }
                Spacer()
                if vaultBusy {
                    ProgressView().tint(Hey.goldInk(scheme))
                } else {
                    // Single-mode headless vault: once sealed it STAYS sealed (switch
                    // disables once on) — background delivery keeps buffering sealed.
                    Toggle("", isOn: vaultBinding).labelsHidden()
                        .tint(Hey.gold).disabled(!vaultable || vaultOn)
                }
            }
            Text(vaultOn
                 ? "Your keys are sealed in hardware and never stored in plaintext. Messages keep arriving in the background — even after a reboot or app update — and decrypt the moment you unlock with \(biometryName)."
                 : vaultable
                   ? "Off: keys are protected by iOS's at-rest encryption + sandbox. Turn on to seal your keys in the Secure Enclave — background delivery keeps working either way."
                   : "Keys are protected by iOS's at-rest encryption + sandbox. Set up Face ID / Touch ID or a passcode to also seal them in hardware.")
                .font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))

            // Reveal recovery phrase — behind a fresh AppLock prompt (Android requireAuth).
            Button {
                Task { await revealPhrase() }
            } label: {
                Label("Reveal recovery phrase", systemImage: "key.fill").font(HeyFont.label)
            }
            .buttonStyle(.bordered).tint(Hey.goldInk(scheme))
            .padding(.top, 4)
        }
        .padding(14).glass()
    }

    private var vaultBinding: Binding<Bool> {
        Binding(
            get: { vaultOn },
            set: { want in
                guard want, !vaultOn else { return }
                Task { await enableVault() }
            }
        )
    }

    // ── Appearance (theme is system-driven on iOS) ──
    private var appearanceRow: some View {
        HStack {
            Text("Appearance").font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))
            Spacer()
            Text("Follows iOS").font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))
        }
        .padding(.horizontal, 2).padding(.top, 4)
    }

    // ── actions ──

    /// Seal the live seed in the Secure Enclave after a fresh AppLock prompt, then
    /// round-trip-verify the seal (seal → unseal == plaintext) before marking the
    /// vault ON — mirrors Android enableVault's safety (never trust an unverified seal).
    private func enableVault() async {
        vaultBusy = true
        defer { vaultBusy = false }
        guard let seed = await store.engine.recoveryPhrase(), !seed.isEmpty else {
            flash("No identity to seal yet"); return
        }
        let ok = await AppLock.prompt(reason: "Seal your Hey identity in hardware")
        guard ok else { return }
        guard IdentityVault.seal(seed) else { flash("Couldn't enable"); return }
        // Round-trip verify (this triggers the biometric ACL once more).
        guard IdentityVault.unseal(reason: "Confirm the seal") == seed else {
            IdentityVault.clear(); flash("Couldn't verify the seal"); return
        }
        IdentityVault.isOn = true
        vaultOn = true
        flash("Keys sealed in hardware")
    }

    /// Require a fresh biometric/passcode, then show the BIP39 phrase. Old devices
    /// without any biometric/passcode reveal directly (matches Android).
    private func revealPhrase() async {
        if AppLock.available() {
            let ok = await AppLock.prompt(reason: "Verify it's you to reveal your recovery phrase")
            guard ok else { return }
        }
        guard let p = await store.engine.recoveryPhrase(), !p.isEmpty else {
            flash("No identity to back up yet"); return
        }
        phrase = p
    }

    private func flash(_ msg: String) {
        toast = msg
        Task {
            try? await Task.sleep(nanoseconds: 1_800_000_000)
            if toast == msg { toast = nil }
        }
    }

    private var biometryName: String {
        switch AppLock.biometryType() {
        case .faceID: return "Face ID"
        case .touchID: return "Touch ID"
        default: return "your passcode"   // .opticID (iOS 17+) / .none → passcode wording
        }
    }
}

/// Identifiable wrapper so the reveal phrase drives a `.sheet(item:)`.
private struct PhraseBox: Identifiable { let text: String; var id: String { text } }

/// The recovery-phrase reveal (port of Android's AlertDialog at 2281-2324). Blocks
/// screenshots/recents in spirit; on iOS we present in a dedicated sheet and copy
/// to the pasteboard with an auto-clear timer.
private struct RecoveryPhraseView: View {
    @Environment(\.colorScheme) private var scheme
    @Environment(\.dismiss) private var dismiss
    let phrase: String
    @State private var copied = false

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    Label { Text("Your recovery phrase").font(HeyFont.header) } icon: {
                        Image(systemName: "key.fill").foregroundStyle(Hey.goldInk(scheme))
                    }
                    .foregroundStyle(Hey.ink(scheme))

                    Text("These 12 words ARE your account — they recover your Hey identity, your Elastos DID, and your wallets (here or in official Elastos Essentials). Anyone with them controls everything. Write them down offline; never share or screenshot.")
                        .font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))
                        .lineSpacing(HeyLineSpacing.caption)

                    Text(phrase)
                        .font(HeyFont.mono(16)).foregroundStyle(Hey.ink(scheme))
                        .lineSpacing(8)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(14)
                        .background(Color.black.opacity(0.13), in: RoundedRectangle(cornerRadius: HeyRadius.attachment, style: .continuous))
                        .privacySensitive()
                }
                .padding(20)
            }
            .scrollContentBackground(.hidden)
            .background(FrostBackground().ignoresSafeArea())
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Done") { dismiss() }.tint(Hey.muted(scheme))
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button(copied ? "Copied" : "Copy") { copy() }
                        .font(HeyFont.label).tint(Hey.goldInk(scheme))
                }
            }
        }
        .tint(Hey.goldInk(scheme))
    }

    /// Copy with a 15s auto-clear and the sensitive flag (so it stays out of the
    /// pasteboard preview / cloud sync). Mirrors Android's EXTRA_IS_SENSITIVE + clear.
    private func copy() {
        let pb = UIPasteboard.general
        if #available(iOS 15.0, *) {
            pb.setItems([[UIPasteboard.typeAutomatic: phrase]],
                        options: [.localOnly: true,
                                  .expirationDate: Date().addingTimeInterval(15)])
        } else {
            pb.string = phrase
        }
        copied = true
        DispatchQueue.main.asyncAfter(deadline: .now() + 15) {
            if pb.string == phrase { pb.items = [] }
        }
    }
}
