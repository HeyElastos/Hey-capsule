import SwiftUI

// 1:1 port of Android FeedScreen (MainActivity.kt:1601-1623): first-load spinner,
// an empty-state, otherwise a scrolling list of PostCards with pull-to-refresh.
// A gold "+" FAB opens the ComposerView (sheet). onOpenProfile is wired by the
// orchestrator (RootView); it defaults to a no-op so this view compiles standalone.
struct FeedView: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var scheme

    var onOpenProfile: (String) -> Void = { _ in }

    @State private var posts: [Post] = []
    @State private var firstLoad = true
    @State private var composing = false

    var body: some View {
        ZStack(alignment: .bottomTrailing) {
            content
            fab
        }
        .task { await load() }
        .sheet(isPresented: $composing) {
            ComposerView(onPosted: { Task { await load() } })
        }
    }

    @ViewBuilder private var content: some View {
        if firstLoad {
            ProgressView()
                .tint(Hey.goldInk(scheme))
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else if posts.isEmpty {
            emptyState
        } else {
            ScrollView {
                LazyVStack(spacing: 0) {
                    ForEach(posts) { p in
                        PostCard(post: p, onOpenProfile: onOpenProfile)
                            .padding(.vertical, 8)
                    }
                }
                .padding(.horizontal, 12)
                .padding(.top, 12)
                .padding(.bottom, 96)
            }
            .scrollContentBackground(.hidden)
            .refreshable { await load() }
        }
    }

    // Empty-state — copy matches Android exactly (MainActivity.kt:1611-1617).
    private var emptyState: some View {
        VStack(spacing: 0) {
            Image(systemName: "camera.fill")
                .font(.system(size: 44)).foregroundStyle(Hey.muted(scheme))
            Spacer().frame(height: 12)
            Text("Your feed is empty")
                .font(.system(size: 17, weight: .semibold)).foregroundStyle(Hey.ink(scheme))
            Text("Tap + to share a photo.")
                .font(HeyFont.body).foregroundStyle(Hey.muted(scheme))
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var fab: some View {
        Button { composing = true } label: {
            Image(systemName: "plus")
                .font(.system(size: 24, weight: .bold))
                .foregroundStyle(Hey.navy)
                .frame(width: 56, height: 56)
                .background(Hey.gold, in: Circle())
                .shadow(color: Hey.gold.opacity(0.35), radius: 10, y: 4)
        }
        .padding(.trailing, 20)
        .padding(.bottom, 96)
    }

    private func load() async {
        posts = (try? await store.engine.feed()) ?? []
        firstLoad = false
    }
}
