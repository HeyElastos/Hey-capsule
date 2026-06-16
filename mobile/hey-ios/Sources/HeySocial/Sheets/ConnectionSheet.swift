import SwiftUI

// Port of Android ConnectionSheet (MainActivity.kt:2340-2526).
//
// "How Hey connects" — the illustrated explainer: relay = matchmaker, carrier =
// direct E2E pipe. Polls carrierHealth() every 2s and shows the live mode
// (direct vs relay-assisted), the connected peer count, the relay in use, and lets
// the user copy their relay / choose a community-or-own relay.
//
// The Android sheet reads a richer health blob (public_v4/v6, udp paths, local_addrs).
// The iOS engine contract's CarrierHealth exposes online/peers/relay/mode, so this
// faithfully renders those; the extra address breakdown is gated on data we have.
struct ConnectionSheet: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var scheme
    @Environment(\.dismiss) private var dismiss

    var onClose: () -> Void = {}

    @State private var health = CarrierHealth()
    // Relay choice persisted locally (the engine reads it on next launch). "" = community.
    @AppStorage("hey_custom_relay", store: UserDefaults(suiteName: AppPaths.appGroup))
    private var customRelay: String = ""
    @State private var relayChoice = "community"
    @State private var relayInput = ""
    @State private var relaySaved = false

    private var direct: Bool { health.mode == "direct" }
    private var online: Bool { health.online }
    private var peers: Int { health.peers }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    header
                    diagram
                    steps
                    liveCard
                    relayCard
                }
                .padding(20)
            }
            .scrollContentBackground(.hidden)
            .background(FrostBackground().ignoresSafeArea())
            .navigationTitle("How Hey connects")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { onClose(); dismiss() }.tint(Hey.goldInk(scheme))
                }
            }
        }
        .tint(Hey.goldInk(scheme))
        .onAppear {
            relayChoice = customRelay.isEmpty ? "community" : "custom"
            relayInput = customRelay
        }
        .task {
            // Poll the carrier health every 2s while the sheet is open.
            while !Task.isCancelled {
                health = await store.engine.carrierHealth()
                try? await Task.sleep(nanoseconds: 2_000_000_000)
            }
        }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text("No servers store your data. Your device is the node.")
                .font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    // ── Diagram: relay introduces (dashed), devices talk directly (solid) ──
    private var diagram: some View {
        ZStack {
            GeometryReader { geo in
                let w = geo.size.width, h = geo.size.height
                let you = CGPoint(x: w * 0.15, y: h * 0.80)
                let friend = CGPoint(x: w * 0.85, y: h * 0.80)
                let relay = CGPoint(x: w * 0.50, y: h * 0.18)
                Path { p in p.move(to: you); p.addLine(to: relay) }
                    .stroke(Hey.muted(scheme).opacity(0.6), style: StrokeStyle(lineWidth: 1.5, dash: [6, 6]))
                Path { p in p.move(to: friend); p.addLine(to: relay) }
                    .stroke(Hey.muted(scheme).opacity(0.6), style: StrokeStyle(lineWidth: 1.5, dash: [6, 6]))
                Path { p in p.move(to: you); p.addLine(to: friend) }
                    .stroke(direct ? Hey.goldInk(scheme) : Hey.muted(scheme).opacity(0.5), lineWidth: 3.5)
            }
            chip("Relay", "point.3.connected.trianglepath.dotted").offset(y: -52)
            chip("You", "iphone").offset(x: -90, y: 38)
            chip("Friend", "iphone").offset(x: 90, y: 38)
            Text(direct ? "direct · encrypted" : "relayed · encrypted")
                .font(HeyFont.tick).foregroundStyle(direct ? Hey.good(scheme) : Hey.muted(scheme))
                .padding(.horizontal, 6).padding(.vertical, 1)
                .background(Hey.sheetBg(scheme), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
                .offset(y: 26)
        }
        .frame(height: 150)
        .padding(8).glass()
    }

    private func chip(_ text: String, _ icon: String) -> some View {
        HStack(spacing: 5) {
            Image(systemName: icon).font(.system(size: 13)).foregroundStyle(Hey.goldInk(scheme))
            Text(text).font(HeyFont.caption).foregroundStyle(Hey.ink(scheme))
        }
        .padding(.horizontal, 8).padding(.vertical, 5)
        .glass(12)
    }

    // ── The three steps ──
    private var steps: some View {
        VStack(spacing: 0) {
            connStep("point.3.connected.trianglepath.dotted", "Relay introduces",
                     "The relay finds your friend's device and helps the two punch through firewalls/NAT. It's a matchmaker — it never stores your account or messages.")
            connStep("arrow.left.arrow.right", "Carrier connects",
                     "Your two devices form a direct peer-to-peer link — the Carrier (iroh). Once joined, messages and media flow device-to-device.")
            connStep("lock.fill", "End-to-end encrypted",
                     "Everything is sealed with ML-KEM-768 + X25519. Even when traffic must pass a relay, it only ever sees ciphertext — never your content.")
        }
    }

    private func connStep(_ icon: String, _ title: String, _ body: String) -> some View {
        HStack(alignment: .top, spacing: 12) {
            ZStack {
                Circle().fill(Hey.avatarGradient).frame(width: 30, height: 30)
                Image(systemName: icon).font(.system(size: 15)).foregroundStyle(Hey.navy)
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(HeyFont.author).foregroundStyle(Hey.ink(scheme))
                Text(body).font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.vertical, 6)
    }

    // ── Live status card ──
    private var liveCard: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                Circle().fill(online ? Hey.good(scheme) : Hey.muted(scheme)).frame(width: 9, height: 9)
                Text(online ? "Live on the carrier · \(peers) connected" : "Connecting to the carrier…")
                    .font(HeyFont.caption)
                    .foregroundStyle(online ? Hey.good(scheme) : Hey.goldInk(scheme))
            }
            Text(direct
                 ? "Direct mode: data is travelling peer-to-peer. The relay is only introducing devices."
                 : "Relay-assisted: this network blocks direct connections, so encrypted data currently rides the relay. It stays end-to-end encrypted, and Hey keeps trying to upgrade to a direct link.")
                .font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))

            if !health.relay.isEmpty {
                Divider().overlay(Hey.glassBorder(scheme)).padding(.vertical, 4)
                Button {
                    UIPasteboard.general.string = health.relay
                } label: {
                    HStack(spacing: 6) {
                        Image(systemName: "globe").font(.system(size: 13)).foregroundStyle(Hey.goldInk(scheme))
                        Text("Relay").font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))
                        Text(health.relay).font(HeyFont.mono(11)).foregroundStyle(Hey.ink(scheme))
                            .lineLimit(1).truncationMode(.middle)
                        Spacer(minLength: 4)
                        Image(systemName: "doc.on.doc").font(.system(size: 11)).foregroundStyle(Hey.muted(scheme))
                    }
                }
                .buttonStyle(.plain)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14).glass()
    }

    // ── Relay / "Hey mesh hub" selection ──
    private var relayCard: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                Image(systemName: "point.3.connected.trianglepath.dotted").foregroundStyle(Hey.goldInk(scheme))
                Text("Relay server").font(HeyFont.author).foregroundStyle(Hey.ink(scheme))
            }
            Text("The relay only introduces peers + carries encrypted data when a direct link isn't possible. Friends on a different relay still reach you — every device is reachable through its own.")
                .font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))

            modeOption("Community relay — Elastos.app (recommended)",
                       "The Elastos community federation relay, with iroh's network as automatic backup. Zero setup.",
                       value: "community") {
                relayChoice = "community"; customRelay = ""; relaySaved = true
            }
            modeOption("My own relay",
                       "Self-hosted hub: paste your relay's address. Nothing about your device touches a third party.",
                       value: "custom") {
                relayChoice = "custom"; relaySaved = false
            }

            if relayChoice == "custom" {
                TextField("https://relay.example.com:8443", text: $relayInput)
                    .font(HeyFont.mono(12)).foregroundStyle(Hey.ink(scheme))
                    .autocorrectionDisabled().textInputAutocapitalization(.never)
                    .padding(12)
                    .background(Hey.glassFill(scheme), in: RoundedRectangle(cornerRadius: HeyRadius.attachment, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: HeyRadius.attachment, style: .continuous)
                        .strokeBorder(Hey.glassBorder(scheme), lineWidth: 1))
                    .onChange(of: relayInput) { _ in relaySaved = false }
                Button {
                    let v = relayInput.trimmingCharacters(in: .whitespacesAndNewlines)
                    if !v.isEmpty { customRelay = v; relaySaved = true }
                } label: {
                    Text("Save").font(HeyFont.label).foregroundStyle(Hey.navy)
                        .frame(maxWidth: .infinity).padding(.vertical, 12)
                        .background(Hey.gold, in: RoundedRectangle(cornerRadius: HeyRadius.attachment, style: .continuous))
                }
            }
            if relaySaved {
                Text("Saved ✓  Fully close + reopen Hey to apply.")
                    .font(HeyFont.caption).foregroundStyle(Hey.good(scheme))
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14).glass()
    }

    private func modeOption(_ title: String, _ body: String, value: String, onSelect: @escaping () -> Void) -> some View {
        let on = relayChoice == value
        return Button(action: onSelect) {
            HStack(alignment: .top, spacing: 10) {
                Image(systemName: on ? "largecircle.fill.circle" : "circle")
                    .foregroundStyle(on ? Hey.goldInk(scheme) : Hey.muted(scheme))
                VStack(alignment: .leading, spacing: 2) {
                    Text(title).font(HeyFont.author).foregroundStyle(Hey.ink(scheme))
                    Text(body).font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))
                }
                Spacer(minLength: 0)
            }
            .padding(12)
            .background((on ? Hey.gold.opacity(0.16) : .clear),
                       in: RoundedRectangle(cornerRadius: HeyRadius.attachment, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: HeyRadius.attachment, style: .continuous)
                .strokeBorder(on ? Hey.goldInk(scheme) : Hey.glassBorder(scheme), lineWidth: 1))
        }
        .buttonStyle(.plain)
    }
}
