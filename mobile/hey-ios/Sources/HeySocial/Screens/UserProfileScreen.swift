import SwiftUI

// Another user's public profile. Port of Android's UserProfileScreen
// (MainActivity.kt:6026-6099): a back row + "Profile" title, large avatar,
// nickname (or short DID), bio, the full DID, a Follow / Message / Tip action row,
// a "Posts (n)" header, and a 3-column grid of their post thumbnails.
//
// Navigation is the orchestrator's: `onBack` and `onMessage(did)` are injected.
// Data comes from the engine's `userProfile(did:)` (aggregated header + counts +
// follow state) and `userPosts(did:)`.
struct UserProfileScreen: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var scheme

    let did: String
    var onBack: () -> Void = {}
    var onMessage: (String) -> Void = { _ in }

    @State private var profile: UserProfile
    @State private var posts: [Post] = []
    @State private var followingThem = false
    @State private var status = ""
    @State private var working = false
    @State private var showTip = false

    private let columns = [GridItem(.flexible(), spacing: 2),
                           GridItem(.flexible(), spacing: 2),
                           GridItem(.flexible(), spacing: 2)]

    init(did: String, onBack: @escaping () -> Void = {}, onMessage: @escaping (String) -> Void = { _ in }) {
        self.did = did
        self.onBack = onBack
        self.onMessage = onMessage
        _profile = State(initialValue: UserProfile(did: did))
    }

    var body: some View {
        ScrollView {
            LazyVStack(spacing: 0) {
                header
                postsGrid
            }
        }
        .scrollContentBackground(.hidden)
        .background(Hey.bg2(scheme).ignoresSafeArea())
        .task { await load() }
        .sheet(isPresented: $showTip) {
            TipSheet(authorDid: did, authorName: displayName, onClose: { showTip = false })
        }
    }

    // MARK: Header

    private var header: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Button(action: onBack) {
                    Image(systemName: "chevron.left")
                        .font(.system(size: 18, weight: .semibold))
                        .foregroundStyle(Hey.ink(scheme))
                }
                .buttonStyle(.plain)
                Text("Profile")
                    .font(HeyFont.author)
                    .foregroundStyle(Hey.ink(scheme))
                Spacer()
            }
            .padding(.bottom, 8)

            Avatar(name: displayName, size: 84, cid: profile.avatar.isEmpty ? nil : profile.avatar)

            Spacer().frame(height: 12)
            Text(displayName)
                .font(HeyFont.subtitle.weight(.bold))
                .foregroundStyle(Hey.ink(scheme))

            if !profile.bio.isEmpty {
                Spacer().frame(height: 4)
                Text(profile.bio)
                    .font(HeyFont.caption)
                    .foregroundStyle(Hey.muted(scheme))
                    .multilineTextAlignment(.center)
                    .padding(.horizontal, 16)
            }

            Spacer().frame(height: 4)
            Text(did)
                .font(HeyFont.timestamp)
                .foregroundStyle(Hey.muted(scheme))
                .lineLimit(1)
                .truncationMode(.middle)
                .padding(.horizontal, 24)

            // Counts (followers / following / posts) — surfaced by userProfile(did:).
            Spacer().frame(height: 10)
            HStack(spacing: 22) {
                stat(profile.posts, "Posts")
                stat(profile.followers, "Followers")
                stat(profile.following, "Following")
            }

            Spacer().frame(height: 14)
            actionRow

            if !status.isEmpty {
                Spacer().frame(height: 8)
                Text(status)
                    .font(HeyFont.caption)
                    .foregroundStyle(Hey.muted(scheme))
            }

            Spacer().frame(height: 16)
            Text("Posts (\(posts.count))")
                .font(HeyFont.author)
                .foregroundStyle(Hey.ink(scheme))
                .frame(maxWidth: .infinity, alignment: .leading)
            Spacer().frame(height: 6)
        }
        .padding(16)
    }

    private var actionRow: some View {
        HStack(spacing: 10) {
            // Follow / Following (toggle via follow/unfollow).
            Button { Task { await toggleFollow() } } label: {
                Text(followingThem ? "Following" : "Follow")
                    .font(HeyFont.label.weight(.bold))
                    .foregroundStyle(Hey.navy)
                    .padding(.horizontal, 18)
                    .padding(.vertical, 9)
                    .background(Hey.gold, in: Capsule())
            }
            .buttonStyle(.plain)
            .disabled(working)

            // Message — opens a chat (startChat then route to the thread).
            Button { Task { await message() } } label: {
                HStack(spacing: 6) {
                    Image(systemName: "bubble.left.and.bubble.right.fill").font(.system(size: 15))
                    Text("Message")
                }
                .font(HeyFont.label)
                .foregroundStyle(Hey.ink(scheme))
                .padding(.horizontal, 16)
                .padding(.vertical, 9)
                .overlay(Capsule().strokeBorder(Hey.glassBorder(scheme), lineWidth: 1))
            }
            .buttonStyle(.plain)
            .disabled(working)

            // Tip — Android opens a TipSheet here (tip by identity over the carrier).
            Button { showTip = true } label: {
                HStack(spacing: 6) {
                    Image(systemName: "dollarsign.circle.fill")
                        .font(.system(size: 15))
                        .foregroundStyle(Hey.goldInk(scheme))
                    Text("Tip")
                }
                .font(HeyFont.label)
                .foregroundStyle(Hey.ink(scheme))
                .padding(.horizontal, 16)
                .padding(.vertical, 9)
                .overlay(Capsule().strokeBorder(Hey.glassBorder(scheme), lineWidth: 1))
            }
            .buttonStyle(.plain)
        }
    }

    private func stat(_ value: Int, _ label: String) -> some View {
        VStack(spacing: 1) {
            Text("\(value)")
                .font(HeyFont.author)
                .foregroundStyle(Hey.ink(scheme))
            Text(label)
                .font(HeyFont.timestamp)
                .foregroundStyle(Hey.muted(scheme))
        }
    }

    // MARK: Posts grid (Android: 3-column thumbnail grid)

    private var postsGrid: some View {
        LazyVGrid(columns: columns, spacing: 2) {
            ForEach(posts) { p in
                PostThumb(post: p)
            }
        }
        .padding(2)
        .padding(.bottom, 96)   // clear the floating dock
    }

    private var displayName: String {
        profile.nickname.isEmpty ? Profile.short(did) : profile.nickname
    }

    // MARK: Data / actions

    private func load() async {
        if let up = try? await store.engine.userProfile(did: did) {
            profile = up
            followingThem = up.isFollowing
        } else if let p = try? await store.engine.profile(did: did) {
            // Fallback to the basic profile + a direct follow-state check.
            profile = UserProfile(did: p.did, nickname: p.nickname, bio: p.bio, avatar: p.avatar)
            followingThem = await store.engine.isFollowing(did: did)
        }
        posts = (try? await store.engine.userPosts(did: did)) ?? []
        profile.posts = posts.count
    }

    private func toggleFollow() async {
        working = true
        defer { working = false }
        do {
            if followingThem {
                try await store.engine.unfollow(did: did)
                followingThem = false
                profile.followers = max(0, profile.followers - 1)
            } else {
                try await store.engine.followBack(did: did)
                followingThem = true
                profile.followers += 1
            }
        } catch {
            status = error.localizedDescription
        }
    }

    private func message() async {
        working = true
        defer { working = false }
        do {
            try await store.engine.startChat(did: did)
            onMessage(did)
        } catch {
            status = error.localizedDescription
        }
    }
}

// MARK: - Grid thumbnail

// One square post tile (Android: aspectRatio 1, rounded 8, photo crop / play icon /
// caption fallback).
private struct PostThumb: View {
    @Environment(\.colorScheme) private var scheme
    let post: Post

    private var photoCid: String? {
        post.media.first { !$0.isVideo }?.cid
    }
    private var hasVideo: Bool {
        post.media.contains { $0.isVideo }
    }

    var body: some View {
        ZStack {
            Color.black.opacity(0.25)
            if let cid = photoCid {
                ContentImage(cid: cid) {
                    Image(systemName: "photo")
                        .font(.system(size: 22))
                        .foregroundStyle(Hey.muted(scheme))
                }
                .scaledToFill()
            } else if hasVideo {
                Image(systemName: "play.circle.fill")
                    .font(.system(size: 28))
                    .foregroundStyle(.white)
            } else {
                Text(String(post.caption.prefix(18)))
                    .font(HeyFont.tick)
                    .foregroundStyle(Hey.muted(scheme))
                    .padding(4)
            }
        }
        .aspectRatio(1, contentMode: .fill)
        .frame(maxWidth: .infinity)
        .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
        .clipped()
    }
}
