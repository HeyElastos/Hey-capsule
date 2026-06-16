import SwiftUI

// Port of Android AboutSheet (MainActivity.kt:2532-2581).
//
// Transparency: what the Elastos Internet OS stack + hey-core actually do for the
// user, in plain language. One item ("Peer-to-peer delivery") shows a LIVE status
// chip read from carrierHealth(), so the empowerment isn't a black box. StrongBox
// language is mapped to "Secure Enclave" for iOS.
struct AboutSheet: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var scheme
    @Environment(\.dismiss) private var dismiss

    var onClose: () -> Void = {}

    @State private var health = CarrierHealth()
    private var online: Bool { health.online }
    private var direct: Bool { health.mode == "direct" }

    private var version: String {
        let v = Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "1.0"
        let b = Bundle.main.infoDictionary?["CFBundleVersion"] as? String ?? "1"
        return "\(v) (\(b))"
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    Text("Built on the Elastos Internet OS — you own your identity, your data, and your connections.")
                        .font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))
                        .padding(.bottom, 16)

                    aboutItem("point.3.connected.trianglepath.dotted", "It runs on your phone",
                              "There is no Hey server. A mini Elastos runtime + the Carrier (the peer-to-peer network) run inside the app, on your device. Your phone is the node — it holds your keys, signs your posts, stores your data, and talks straight to your friends' phones.")
                    aboutItem("person.text.rectangle.fill", "You own your identity",
                              "Your identity is a self-sovereign did:key — a keypair only your device holds. No email, no phone number, no account on someone's server. It signs everything you create so others can verify it's really you.")
                    aboutItem("lock.fill", "Private by cryptography",
                              "Messages and media are end-to-end encrypted with post-quantum crypto (ML-KEM-768 + X25519, ChaCha20-Poly1305). Even relays only ever see ciphertext — never your content.")
                    // Live network mode — reflects how THIS phone is connected right now.
                    aboutItemLive("arrow.left.arrow.right", "Peer-to-peer delivery",
                                  body: !online
                                      ? "Connecting to the carrier…"
                                      : direct
                                        ? "Right now your phone is connected DIRECTLY — data flows device-to-device and the relay is only used to introduce peers."
                                        : "Right now data rides the encrypted relay (this network blocks a direct link). It stays end-to-end encrypted, and Hey keeps trying to upgrade to a direct link.",
                                  chip: !online ? "○ Connecting" : (direct ? "● Direct P2P" : "● Relay-assisted"),
                                  chipColor: !online ? Hey.muted(scheme) : (direct ? Hey.good(scheme) : Hey.goldInk(scheme)))
                    aboutItem("shield.lefthalf.filled", "Sandboxed & on-device",
                              "All your keys and data live in Hey's private app container, sandboxed by iOS so other apps can't read them. Nothing is uploaded to a company. Hardware-backed encryption (the Secure Enclave) and an optional Face ID / Touch ID lock add another layer.")
                    aboutItem("globe", "No lock-in",
                              "hey-core is the same engine across phone, web and desktop, speaking open Elastos interfaces. Your identity and social graph are yours to take anywhere.")

                    Text("hey-core · Elastos Carrier (iroh) · IPFS content store · did:key identity")
                        .font(HeyFont.timestamp).foregroundStyle(Hey.muted(scheme))
                        .padding(.top, 8)
                    Text("Version \(version)")
                        .font(HeyFont.timestamp).foregroundStyle(Hey.muted(scheme))
                        .padding(.top, 4)
                }
                .padding(20)
            }
            .scrollContentBackground(.hidden)
            .background(FrostBackground().ignoresSafeArea())
            .navigationTitle("About Hey")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { onClose(); dismiss() }.tint(Hey.goldInk(scheme))
                }
            }
        }
        .tint(Hey.goldInk(scheme))
        .task {
            while !Task.isCancelled {
                health = await store.engine.carrierHealth()
                try? await Task.sleep(nanoseconds: 2_000_000_000)
            }
        }
    }

    private func aboutItem(_ icon: String, _ title: String, _ body: String) -> some View {
        HStack(alignment: .top, spacing: 12) {
            iconBadge(icon)
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.system(size: 15, weight: .semibold)).foregroundStyle(Hey.ink(scheme))
                Text(body).font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.vertical, 7)
    }

    private func aboutItemLive(_ icon: String, _ title: String, body: String, chip: String, chipColor: Color) -> some View {
        HStack(alignment: .top, spacing: 12) {
            iconBadge(icon)
            VStack(alignment: .leading, spacing: 2) {
                HStack(alignment: .top) {
                    Text(title).font(.system(size: 15, weight: .semibold)).foregroundStyle(Hey.ink(scheme))
                    Spacer(minLength: 6)
                    Text(chip).font(HeyFont.timestamp).foregroundStyle(chipColor)
                        .padding(.horizontal, 8).padding(.vertical, 2)
                        .background(Hey.glassFill(scheme), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                        .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous)
                            .strokeBorder(Hey.glassBorder(scheme), lineWidth: 1))
                }
                Text(body).font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.vertical, 7)
    }

    private func iconBadge(_ icon: String) -> some View {
        ZStack {
            Circle().fill(Hey.avatarGradient).frame(width: 34, height: 34)
            Image(systemName: icon).font(.system(size: 16)).foregroundStyle(Hey.navy)
        }
    }
}
