import SwiftUI
import UIKit

// "Creating your Elastos identity" → "Your Elastos DID is ready" — the iOS port of
// Android's DidGenerationScreen (MainActivity.kt:3581-3671). A pulsing fingerprint with
// a four-step checklist that fills in, then reveals the derived did:elastos with a
// copy-to-clipboard chip and an "Enter wallet" button that calls onDone.
//
// The DID is read from the engine (elastosDid) — it's already derived on this device by
// the time this screen shows; the staged checklist is purely the reassuring animation.
struct DidGenerationView: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var scheme

    var buttonLabel: String = "Enter wallet"
    let onDone: () -> Void

    @State private var step = 0
    @State private var did: String?
    @State private var revealed = false
    @State private var pulse = false
    @State private var copied = false

    // (label, SF Symbol) — mirrors the Android steps + icons.
    private let steps: [(String, String)] = [
        ("Deriving your keys", "key.fill"),
        ("Creating your Elastos DID", "person.text.rectangle.fill"),
        ("Setting up your wallets", "wallet.pass.fill"),
        ("Securing on this device", "shield.fill"),
    ]

    var body: some View {
        ZStack {
            FrostBackground()
            VStack(spacing: 0) {
                if !revealed { generating } else { ready }
            }
            .padding(.horizontal, 32)
            .padding(.bottom, 110)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .task { await run() }
    }

    // ── generating ──
    private var generating: some View {
        VStack(spacing: 0) {
            ZStack {
                Circle().fill(Hey.avatarGradient).frame(width: 108, height: 108)
                Image(systemName: "touchid").font(.system(size: 52)).foregroundStyle(Hey.navy)
            }
            .scaleEffect(pulse ? 1.10 : 1.0)
            .animation(.easeInOut(duration: 0.9).repeatForever(autoreverses: true), value: pulse)
            .onAppear { pulse = true }

            Spacer().frame(height: 24)
            Text("Creating your Elastos identity").font(HeyFont.subtitle.weight(.bold)).foregroundStyle(Hey.ink(scheme))
            Spacer().frame(height: 6)
            Text("One DID for your wallets — derived on this device, in a second.")
                .font(HeyFont.caption).foregroundStyle(Hey.muted(scheme)).multilineTextAlignment(.center)
            Spacer().frame(height: 28)

            VStack(alignment: .leading, spacing: 14) {
                ForEach(Array(steps.enumerated()), id: \.offset) { i, entry in
                    HStack(spacing: 12) {
                        ZStack {
                            if i < step {
                                Image(systemName: "checkmark.circle.fill").font(.system(size: 22))
                                    .foregroundStyle(Hey.good(scheme))
                            } else if i == step {
                                ProgressView().scaleEffect(0.8).tint(Hey.goldInk(scheme))
                            } else {
                                Image(systemName: entry.1).font(.system(size: 20))
                                    .foregroundStyle(Hey.muted(scheme).opacity(0.5))
                            }
                        }
                        .frame(width: 24, height: 24)
                        Text(entry.0)
                            .font(.system(size: 14, weight: i == step ? .semibold : .regular))
                            .foregroundStyle(i <= step ? Hey.ink(scheme) : Hey.muted(scheme))
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(18).glass()
        }
    }

    // ── ready: reveal the DID + Enter wallet ──
    private var ready: some View {
        VStack(spacing: 0) {
            ZStack {
                Circle().fill(Hey.avatarGradient).frame(width: 96, height: 96)
                Image(systemName: "checkmark.seal.fill").font(.system(size: 48)).foregroundStyle(Hey.navy)
            }
            Spacer().frame(height: 22)
            Text("Your Elastos DID is ready").font(HeyFont.subtitle.weight(.bold)).foregroundStyle(Hey.ink(scheme))
            Spacer().frame(height: 10)

            if let d = did {
                Button { copy(d) } label: {
                    HStack(spacing: 8) {
                        Text(shortDid(d)).font(HeyFont.mono(13)).foregroundStyle(Hey.goldInk(scheme))
                        Image(systemName: copied ? "checkmark" : "doc.on.doc")
                            .font(.system(size: 14)).foregroundStyle(Hey.muted(scheme))
                    }
                    .padding(.horizontal, 12).padding(.vertical, 10)
                    .background(Hey.glassFill(scheme),
                                in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                }
            }

            Spacer().frame(height: 14)
            Text("Recover it anytime in official Elastos Essentials with your recovery phrase. It manages your ELA + ESC wallets.")
                .font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))
                .multilineTextAlignment(.center).lineSpacing(19 - 13)
            Spacer().frame(height: 28)

            Button(action: onDone) {
                Text(buttonLabel).font(.system(size: 16, weight: .bold)).foregroundStyle(Hey.navy)
                    .frame(maxWidth: .infinity).frame(height: 52)
                    .background(Hey.gold, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            }
        }
    }

    // ── derive + staged reveal (LaunchedEffect port) ──
    private func run() async {
        let derived = await store.engine.elastosDid()
        for i in steps.indices {
            try? await Task.sleep(nanoseconds: 620_000_000)
            await MainActor.run { step = i + 1 }
        }
        try? await Task.sleep(nanoseconds: 280_000_000)
        await MainActor.run {
            did = derived
            withAnimation { revealed = true }
        }
    }

    // did:elastos:abcd1234…uvwxyz  (matches the Android take(8)…takeLast(6) shape)
    private func shortDid(_ d: String) -> String {
        let body = d.hasPrefix("did:elastos:")
            ? String(d.dropFirst("did:elastos:".count))
            : d
        guard body.count > 14 else { return d }
        return "did:elastos:\(body.prefix(8))…\(body.suffix(6))"
    }

    private func copy(_ d: String) {
        UIPasteboard.general.string = d
        copied = true
        Task {
            try? await Task.sleep(nanoseconds: 1_500_000_000)
            await MainActor.run { copied = false }
        }
    }
}
