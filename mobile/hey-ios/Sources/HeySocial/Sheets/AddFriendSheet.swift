import SwiftUI

// Port of Android AddFriendSheet (MainActivity.kt:2762-2820), extended with the
// following/followers lists this group owns (group notes).
//
// Follow someone by their Hey friend link or QR payload. A bare DID carries no PQ
// keys, so it can't open a private channel — the sheet rejects it and asks for the
// friend link/QR (which bundles the encryption keys + ticket), exactly like Android.
// Below the input it shows who you follow / who follows you, with Follow-back.
struct AddFriendSheet: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var scheme
    @Environment(\.dismiss) private var dismiss

    var onClose: () -> Void = {}
    var onFollowed: () -> Void = {}
    var onOpenProfile: (String) -> Void = { _ in }

    @State private var input = ""
    @State private var status = ""
    @State private var busy = false

    @State private var following: [Follow] = []
    @State private var followers: [Follow] = []
    @State private var loadingLists = true

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    Text("Paste their Hey friend link, or scan their QR.")
                        .font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))

                    // singleLine input — a long pasted/scanned link stays one row.
                    TextField("hey:follow:…", text: $input, axis: .horizontal)
                        .font(HeyFont.mono(13)).foregroundStyle(Hey.ink(scheme))
                        .autocorrectionDisabled().textInputAutocapitalization(.never)
                        .padding(12)
                        .background(Hey.glassFill(scheme), in: RoundedRectangle(cornerRadius: HeyRadius.attachment, style: .continuous))
                        .overlay(RoundedRectangle(cornerRadius: HeyRadius.attachment, style: .continuous)
                            .strokeBorder(Hey.glassBorder(scheme), lineWidth: 1))

                    if input.count > 24 {
                        Text("✓ Link ready (\(input.count) chars)")
                            .font(HeyFont.caption).foregroundStyle(Hey.good(scheme))
                    }

                    HStack(spacing: 12) {
                        Button {
                            if let s = UIPasteboard.general.string { input = s.trimmingCharacters(in: .whitespacesAndNewlines) }
                        } label: {
                            Label("Paste", systemImage: "doc.on.clipboard").font(HeyFont.label)
                        }
                        .buttonStyle(.bordered).tint(Hey.goldInk(scheme))

                        Button { Task { await doFollow() } } label: {
                            if busy {
                                ProgressView().tint(Hey.navy).frame(maxWidth: .infinity)
                            } else {
                                Text("Follow").font(HeyFont.label).foregroundStyle(Hey.navy).frame(maxWidth: .infinity)
                            }
                        }
                        .padding(.vertical, 10)
                        .background(Hey.gold, in: RoundedRectangle(cornerRadius: HeyRadius.attachment, style: .continuous))
                        .disabled(busy)
                    }

                    if !status.isEmpty {
                        Text(status).font(HeyFont.callout).foregroundStyle(Hey.muted(scheme))
                    }

                    // ── Following / followers ──
                    if loadingLists {
                        ProgressView().tint(Hey.goldInk(scheme)).frame(maxWidth: .infinity).padding(.top, 8)
                    } else {
                        if !following.isEmpty {
                            sectionTitle("Following")
                            ForEach(following) { f in personRow(f, isFollower: false) }
                        }
                        if !followers.isEmpty {
                            sectionTitle("Followers")
                            ForEach(followers) { f in personRow(f, isFollower: true) }
                        }
                        if following.isEmpty && followers.isEmpty {
                            Text("Follow someone above to grow your circle.")
                                .font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))
                                .frame(maxWidth: .infinity, alignment: .center).padding(.top, 12)
                        }
                    }
                }
                .padding(20)
            }
            .scrollContentBackground(.hidden)
            .background(FrostBackground().ignoresSafeArea())
            .navigationTitle("Follow someone")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { onClose(); dismiss() }.tint(Hey.goldInk(scheme))
                }
            }
        }
        .tint(Hey.goldInk(scheme))
        .task { await loadLists() }
    }

    private func sectionTitle(_ t: String) -> some View {
        Text(t).font(HeyFont.label).foregroundStyle(Hey.muted(scheme)).padding(.top, 8)
    }

    private func personRow(_ f: Follow, isFollower: Bool) -> some View {
        HStack(spacing: 12) {
            Avatar(name: Profile.short(f.did), size: 40)
            Text(Profile.short(f.did)).font(HeyFont.author).foregroundStyle(Hey.ink(scheme))
                .lineLimit(1)
            Spacer()
            // A follower you don't yet follow → Follow back.
            if isFollower && !following.contains(where: { $0.did == f.did }) {
                Button { Task { await followBack(f.did) } } label: {
                    Text("Follow back").font(HeyFont.label).foregroundStyle(Hey.navy)
                        .padding(.horizontal, 12).padding(.vertical, 7)
                        .background(Hey.gold, in: Capsule())
                }
            } else {
                Image(systemName: "chevron.right").font(.system(size: 12)).foregroundStyle(Hey.muted(scheme))
            }
        }
        .contentShape(Rectangle())
        .onTapGesture { onOpenProfile(f.did) }
        .padding(12).glass()
    }

    // ── actions ──

    /// Follow by friend link / QR payload. Rejects a bare DID (no PQ keys → no private
    /// channel), mirroring Android doFollow's guard exactly.
    private func doFollow() async {
        let v = input.trimmingCharacters(in: .whitespacesAndNewlines)
        if v.isEmpty { status = "Paste a Hey friend link or scan a QR"; return }
        if v.hasPrefix("did:") && !v.contains("hey:follow") {
            status = "That's a DID — it can't start a private channel. Ask them for their Hey friend link or QR."
            return
        }
        busy = true; status = "Connecting…"
        do {
            try await store.engine.follow(v)
            busy = false
            input = ""
            status = ""
            await loadLists()
            onFollowed()
        } catch {
            busy = false
            status = "Failed: \(error.localizedDescription)"
        }
    }

    private func followBack(_ did: String) async {
        do {
            try await store.engine.followBack(did: did)
            await loadLists()
            onFollowed()
        } catch {
            status = "Failed: \(error.localizedDescription)"
        }
    }

    private func loadLists() async {
        loadingLists = true
        following = (try? await store.engine.following()) ?? []
        followers = (try? await store.engine.followers()) ?? []
        loadingLists = false
    }
}
