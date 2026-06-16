import SwiftUI

// SHARED component — owned by group "feed", reused by "activity".
// 1:1 port of Android PostCard (MainActivity.kt:1641-1770): avatar + name + time
// header (tap name → onOpenProfile), media carousel, like row, comment count that
// opens CommentsSheet, and an own-post menu (edit caption / delete) via long-press
// or the ⋯ button. The like toggle + reaction/comment counts come from the engine.
//
// Signature is locked: PostCard(post:onOpenProfile:). The "is this mine?" check
// reads store.me?.did (Android passes myDid; here it's in the environment).
struct PostCard: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var scheme

    let post: Post
    var onOpenProfile: (String) -> Void

    @State private var reactions = Reactions.empty
    @State private var commentCount = 0
    @State private var menu = false
    @State private var editing = false
    @State private var editText = ""
    @State private var showComments = false
    @State private var showTip = false

    private var mine: Bool { post.author == (store.me?.did ?? "") }
    private var displayName: String {
        post.authorName.isEmpty ? HeyShort.did(post.author) : post.authorName
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            if !post.media.isEmpty {
                Spacer().frame(height: 10)
                MediaCarousel(media: post.media.map(\.cid))
                Spacer().frame(height: 10)
            }
            likeRow
            if !post.caption.isEmpty {
                Spacer().frame(height: 8)
                Text(post.caption).font(HeyFont.body).foregroundStyle(Hey.ink(scheme))
            }
        }
        .padding(14)
        .glass()
        .contentShape(Rectangle())
        .onLongPressGesture { if mine { menu = true } }
        .task(id: post.id) { await loadCounts() }
        // Own-post menu — Android shows this as a DropdownMenu on the ⋯ button / long-press.
        .confirmationDialog("Post", isPresented: $menu, titleVisibility: .hidden) {
            Button("Edit caption") { editText = post.caption; editing = true }
            Button("Delete post", role: .destructive) {
                Task { try? await store.engine.deletePost(id: post.id) }
            }
            Button("Cancel", role: .cancel) {}
        }
        // Edit-caption dialog (Android AlertDialog, MainActivity.kt:1751-1768).
        .alert("Edit caption", isPresented: $editing) {
            TextField("Caption", text: $editText)
            Button("Save") {
                Task { try? await store.engine.editPost(id: post.id, caption: editText) }
            }
            Button("Cancel", role: .cancel) {}
        }
        .sheet(isPresented: $showComments) {
            CommentsSheet(post: post, onChanged: { Task { await loadCounts() } })
                .presentationDetents([.medium, .large])
        }
    }

    // MARK: header
    private var header: some View {
        HStack(spacing: 10) {
            Avatar(name: displayName, size: 36, cid: post.authorAvatar)
                .onTapGesture { if !mine { onOpenProfile(post.author) } }
            VStack(alignment: .leading, spacing: 1) {
                Text(displayName).font(HeyFont.author).foregroundStyle(Hey.ink(scheme))
                if post.ts > 0 {
                    Text(RelativeTime.short(post.ts)).font(HeyFont.timestamp).foregroundStyle(Hey.muted(scheme))
                }
            }
            .contentShape(Rectangle())
            .onTapGesture { if !mine { onOpenProfile(post.author) } }
            Spacer()
            if mine {
                Button { menu = true } label: {
                    Image(systemName: "ellipsis").font(.system(size: 18)).foregroundStyle(Hey.muted(scheme))
                }
            }
        }
    }

    // MARK: like / comment / tip row
    private var likeRow: some View {
        HStack(spacing: 0) {
            Button {
                Task {
                    if let r = try? await store.engine.toggleLike(postId: post.id) { reactions = r }
                }
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: reactions.liked ? "heart.fill" : "heart")
                        .font(.system(size: 24))
                        .foregroundStyle(reactions.liked ? Hey.like : Hey.ink(scheme))
                    if reactions.likeCount > 0 {
                        Text("\(reactions.likeCount)").font(.system(size: 14)).foregroundStyle(Hey.ink(scheme))
                    }
                }
            }
            .buttonStyle(.plain)

            Spacer().frame(width: 18)

            Button { showComments = true } label: {
                HStack(spacing: 6) {
                    Image(systemName: "bubble.right")
                        .font(.system(size: 22))
                        .foregroundStyle(Hey.ink(scheme))
                    if commentCount > 0 {
                        Text("\(commentCount)").font(.system(size: 14)).foregroundStyle(Hey.ink(scheme))
                    }
                }
            }
            .buttonStyle(.plain)

            Spacer()

            // Tip the author by identity (resolved via their profile) — Android opens TipSheet.
            if !mine {
                Button { showTip = true } label: {
                    Image(systemName: "dollarsign.circle.fill")
                        .font(.system(size: 22))
                        .foregroundStyle(Hey.goldInk(scheme))
                }
                .buttonStyle(.plain)
            }
        }
        .sheet(isPresented: $showTip) {
            // The shared wallet/tipping sheet — tip the author by identity over the carrier.
            TipSheet(authorDid: post.author, authorName: displayName, onClose: { showTip = false })
        }
    }

    private func loadCounts() async {
        if let r = try? await store.engine.reactions(postId: post.id) { reactions = r }
        if let c = try? await store.engine.comments(postId: post.id) { commentCount = c.count }
    }
}

/// Short DID label — mirror of HeyApi.shortDid (removePrefix "did:key:z", take 10, …).
enum HeyShort {
    static func did(_ did: String) -> String {
        String(did.replacingOccurrences(of: "did:key:z", with: "").prefix(10)) + "…"
    }
}
