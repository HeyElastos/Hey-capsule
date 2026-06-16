import SwiftUI

/// My profile tab — header (avatar / name / bio), carrier status, edit + QR entry,
/// security + connection + about cards, appearance toggle, follow lists, and my
/// posts (port of ProfileScreen, MainActivity.kt:2030-2166).
///
/// Navigation is the orchestrator's: settings gear, opening a person, and the
/// connection/about sheets are passed in as closures. The Edit and QR sheets are
/// OWNED here and presented locally.
struct ProfileView: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var scheme

    var online: Bool = false
    var peers: Int = 0
    var onOpenSettings: () -> Void = {}
    var onOpenProfile: (String) -> Void = { _ in }
    var onEdit: () -> Void = {}
    var onShowQr: () -> Void = {}

    @State private var me = Profile(did: "")
    @State private var following: [Follow] = []
    @State private var followers: [Follow] = []
    @State private var chats = 0
    @State private var myPosts: [Post] = []

    @State private var showEdit = false
    @State private var showQr = false

    private var ink: Color { Hey.ink(scheme) }
    private var muted: Color { Hey.muted(scheme) }
    private var good: Color { Hey.good(scheme) }
    private var goldInk: Color { Hey.goldInk(scheme) }

    var body: some View {
        ScrollView {
            VStack(spacing: 0) {
                // settings gear, top-right
                HStack {
                    Spacer()
                    Button(action: onOpenSettings) {
                        Image(systemName: "gearshape.fill").foregroundStyle(ink)
                    }
                }

                Avatar(name: me.nickname.isEmpty ? "You" : me.nickname, size: 88, cid: me.avatar)
                    .padding(.top, 2)
                Spacer().frame(height: 14)
                Text(me.nickname.isEmpty ? "You" : me.nickname)
                    .font(.system(size: 22, weight: .bold)).foregroundStyle(ink)
                if !me.bio.isEmpty {
                    Spacer().frame(height: 4)
                    Text(me.bio).font(HeyFont.callout).foregroundStyle(muted)
                        .multilineTextAlignment(.center)
                }
                Spacer().frame(height: 6)
                Text(online ? "Carrier online · \(peers) peers" : "Carrier connecting…")
                    .font(HeyFont.caption).foregroundStyle(online ? good : goldInk)

                Spacer().frame(height: 10)
                Button { showEdit = true; onEdit() } label: {
                    HStack(spacing: 6) {
                        Image(systemName: "pencil").font(.system(size: 16))
                        Text("Edit profile")
                    }
                    .font(HeyFont.label)
                    .foregroundStyle(ink)
                    .padding(.vertical, 8).padding(.horizontal, 16)
                    .overlay(Capsule().strokeBorder(Hey.glassBorder(scheme), lineWidth: 1))
                }

                Spacer().frame(height: 14)
                // stat badges
                HStack(spacing: 8) {
                    badge(online ? "● Online" : "○ Offline", online ? good : muted)
                    badge("\(followers.count) followers", goldInk)
                    badge("\(following.count) following", ink)
                    badge("\(chats) chats", ink)
                }

                Spacer().frame(height: 14)
                securityCard
                Spacer().frame(height: 14)
                aboutRow
                Spacer().frame(height: 14)
                appearanceRow

                Spacer().frame(height: 14)
                // Connecting is link/QR only — a bare DID can't open a private channel.
                Button { showQr = true; onShowQr() } label: {
                    HStack(spacing: 6) {
                        Image(systemName: "qrcode").font(.system(size: 18))
                        Text("Share my invite QR")
                    }
                    .font(HeyFont.label)
                    .foregroundStyle(ink)
                    .frame(maxWidth: .infinity, minHeight: 44)
                    .overlay(
                        RoundedRectangle(cornerRadius: HeyRadius.attachment, style: .continuous)
                            .strokeBorder(Hey.glassBorder(scheme), lineWidth: 1)
                    )
                }

                if !followers.isEmpty {
                    Spacer().frame(height: 18)
                    sectionHeader("Followers (\(followers.count))")
                    Spacer().frame(height: 8)
                    ForEach(followers) { f in
                        PersonRow(did: f.did, onTap: { onOpenProfile(f.did) })
                    }
                }

                Spacer().frame(height: 18)
                sectionHeader("Following (\(following.count))")
                Spacer().frame(height: 8)
                ForEach(following) { f in
                    PersonRow(did: f.did, onTap: { onOpenProfile(f.did) }) {
                        Button("Unfollow") { unfollow(f.did) }
                            .font(HeyFont.timestamp).foregroundStyle(muted)
                    }
                }

                if !myPosts.isEmpty {
                    Spacer().frame(height: 18)
                    sectionHeader("My posts (\(myPosts.count))")
                    Spacer().frame(height: 8)
                    ForEach(myPosts) { p in
                        PostCard(post: p, onOpenProfile: onOpenProfile)
                        Spacer().frame(height: 14)
                    }
                }

                Spacer().frame(height: 96)
            }
            .padding(.horizontal, 20).padding(.top, 12)
        }
        .scrollContentBackground(.hidden)
        .task { await reload() }
        .refreshable { await reload() }
        .sheet(isPresented: $showEdit) {
            EditProfileSheet(current: me) {
                showEdit = false
                Task { await reload() }
            }
        }
        .sheet(isPresented: $showQr) {
            MyQrSheet(did: me.did)
        }
    }

    // MARK: cards

    private var securityCard: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 8) {
                Image(systemName: "checkmark.shield.fill").font(.system(size: 20)).foregroundStyle(good)
                Text("Security").font(HeyFont.author).foregroundStyle(ink)
            }
            Spacer().frame(height: 8)
            secRow("Encryption", "End-to-end · ML-KEM-768 + X25519")
            secRow("Keys", "Held on this device, never uploaded")
            secRow("Identity", "Self-sovereign did:key — owned by you")
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14).glass()
    }

    private var aboutRow: some View {
        HStack(spacing: 8) {
            Image(systemName: "info.circle.fill").font(.system(size: 20)).foregroundStyle(goldInk)
            Text("About Hey").font(HeyFont.author).foregroundStyle(ink)
            Spacer()
            Image(systemName: "chevron.right").foregroundStyle(muted)
        }
        .padding(14)
        .frame(maxWidth: .infinity)
        .glass()
    }

    private var appearanceRow: some View {
        HStack {
            Text("Appearance").font(HeyFont.caption).foregroundStyle(muted)
            Spacer()
            // System-driven on iOS; the dock/theme follow the OS appearance.
            Image(systemName: scheme == .dark ? "moon.fill" : "sun.max.fill")
                .foregroundStyle(goldInk)
        }
    }

    // MARK: bits

    private func badge(_ text: String, _ color: Color) -> some View {
        Text(text)
            .font(HeyFont.timestamp.weight(.semibold))
            .foregroundStyle(color)
            .padding(.vertical, 5).padding(.horizontal, 10)
            .background(Hey.glassFill(scheme), in: Capsule())
            .overlay(Capsule().strokeBorder(Hey.glassBorder(scheme), lineWidth: 1))
    }

    private func secRow(_ label: String, _ value: String) -> some View {
        HStack(alignment: .top, spacing: 8) {
            Text(label).font(HeyFont.caption).foregroundStyle(muted).frame(width: 90, alignment: .leading)
            Text(value).font(HeyFont.caption).foregroundStyle(ink)
            Spacer(minLength: 0)
        }
        .padding(.vertical, 3)
    }

    private func sectionHeader(_ title: String) -> some View {
        Text(title)
            .font(HeyFont.author)
            .foregroundStyle(ink)
            .frame(maxWidth: .infinity, alignment: .leading)
    }

    // MARK: data

    private func reload() async {
        async let prof = try? store.engine.profile()
        async let fl = try? store.engine.following()
        async let fr = try? store.engine.followers()
        async let cs = try? store.engine.chats()
        let p = await prof ?? me
        me = p
        following = await fl ?? []
        followers = await fr ?? []
        chats = (await cs)?.count ?? 0
        myPosts = (try? await store.engine.userPosts(did: p.did)) ?? []
    }

    private func unfollow(_ did: String) {
        Task {
            try? await store.engine.unfollow(did: did)
            await reload()
        }
    }
}
