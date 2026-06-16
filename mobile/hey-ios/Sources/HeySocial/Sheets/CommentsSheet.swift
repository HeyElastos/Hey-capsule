import SwiftUI

// Comments for a post — threaded by parent, with an add-comment field.
// Ports Android's inline comment block + CommentRow (MainActivity.kt:1716-1748,
// 1851-1861): top-level comments render flush, replies indent under their parent,
// each top-level row has a "Reply" affordance, and the write field shows a
// "Replying to …" banner + Cancel when a reply is in progress.
//
// In Android this lived inside PostCard; the iOS feed group presents it as a sheet
// (opened from the comment count). onChanged lets the caller refresh the post's count.
struct CommentsSheet: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.dismiss) private var dismiss
    @Environment(\.colorScheme) private var scheme

    let post: Post
    var onChanged: () -> Void = {}

    @State private var comments: [Comment] = []
    @State private var commentText = ""
    @State private var replyTo: Comment? = nil
    @State private var loading = true
    @FocusState private var fieldFocused: Bool

    private var topLevel: [Comment] { comments.filter { $0.parent.isEmpty } }
    private func replies(of c: Comment) -> [Comment] { comments.filter { $0.parent == c.id } }

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                listBody
                Divider().background(Hey.glassBorder(scheme))
                if let r = replyTo { replyBanner(r) }
                writeField
            }
            .background(Hey.sheetBg(scheme).ignoresSafeArea())
            .navigationTitle("Comments")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button("Done") { dismiss() }.tint(Hey.goldInk(scheme))
                }
            }
        }
        .task { await load() }
    }

    @ViewBuilder private var listBody: some View {
        if loading {
            ProgressView().tint(Hey.goldInk(scheme))
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else if comments.isEmpty {
            VStack(spacing: 6) {
                Image(systemName: "bubble.left").font(.system(size: 40)).foregroundStyle(Hey.muted(scheme))
                Text("No comments yet").font(HeyFont.body).foregroundStyle(Hey.muted(scheme))
                Text("Be the first to say something.").font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(topLevel) { c in
                        CommentRow(comment: c, indent: false) {
                            replyTo = c; fieldFocused = true
                        }
                        ForEach(replies(of: c)) { r in
                            CommentRow(comment: r, indent: true) {
                                replyTo = c; fieldFocused = true
                            }
                        }
                    }
                }
                .padding(.horizontal, 16).padding(.vertical, 12)
            }
        }
    }

    private func replyBanner(_ r: Comment) -> some View {
        HStack {
            Text("Replying to \(r.authorName.isEmpty ? HeyShort.did(r.author) : r.authorName)")
                .font(HeyFont.timestamp).foregroundStyle(Hey.goldInk(scheme))
            Spacer()
            Button("Cancel") { replyTo = nil }
                .font(HeyFont.timestamp).tint(Hey.muted(scheme))
        }
        .padding(.horizontal, 16).padding(.top, 8)
    }

    private var writeField: some View {
        HStack(spacing: 8) {
            TextField(replyTo != nil ? "Reply…" : "Add a comment…", text: $commentText, axis: .vertical)
                .focused($fieldFocused)
                .font(.system(size: 14)).foregroundStyle(Hey.ink(scheme))
                .tint(Hey.gold)
                .padding(10)
                .background(Hey.glassFill(scheme), in: RoundedRectangle(cornerRadius: HeyRadius.attachment, style: .continuous))
                .overlay(
                    RoundedRectangle(cornerRadius: HeyRadius.attachment, style: .continuous)
                        .strokeBorder(Hey.glassBorder(scheme), lineWidth: 1)
                )
            Button("Send") { Task { await send() } }
                .font(HeyFont.label).tint(Hey.goldInk(scheme))
                .disabled(commentText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        }
        .padding(.horizontal, 16).padding(.vertical, 10)
    }

    private func load() async {
        comments = (try? await store.engine.comments(postId: post.id)) ?? []
        loading = false
    }

    private func send() async {
        let t = commentText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !t.isEmpty else { return }
        let parent = replyTo?.id ?? ""
        commentText = ""; replyTo = nil; fieldFocused = false
        _ = try? await store.engine.addComment(postId: post.id, text: t, parent: parent)
        await load()
        onChanged()
    }
}

// One comment line: gold author name + ink text; replies indent (Android CommentRow,
// MainActivity.kt:1851-1861). Top-level rows show a "Reply" button.
private struct CommentRow: View {
    @Environment(\.colorScheme) private var scheme
    let comment: Comment
    let indent: Bool
    var onReply: () -> Void

    private var name: String { comment.authorName.isEmpty ? HeyShort.did(comment.author) : comment.authorName }

    var body: some View {
        HStack(alignment: .top, spacing: 0) {
            (Text(name + "  ").font(.system(size: 13, weight: .semibold)).foregroundColor(Hey.goldInk(scheme))
             + Text(comment.text).font(.system(size: 14)).foregroundColor(Hey.ink(scheme)))
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
            if !indent {
                Button("Reply") { onReply() }
                    .font(HeyFont.timestamp).tint(Hey.muted(scheme))
            }
        }
        .padding(.leading, indent ? 26 : 0)
        .padding(.top, 3)
    }
}
