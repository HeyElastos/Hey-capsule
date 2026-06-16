import SwiftUI

// Feed media: a swipeable pager with "i/n" + dots, tap → full-screen pinch/pan viewer
// (Android: MediaCarousel + FullImageViewer, MainActivity.kt:1584-1650, 4862).
//
// Media is addressed by namespace (cid), never by network. Real image bytes come
// from the content provider; until that loader is wired, tiles show a branded
// placeholder so the layout/feel is correct. Replace `MediaTile` body with an
// AsyncImage over a custom URLProtocol that resolves `cid` via the engine.
struct MediaCarousel: View {
    let media: [String]            // content CIDs
    @State private var index = 0
    @State private var viewing: Int? = nil

    var body: some View {
        if media.isEmpty { EmptyView() } else {
            ZStack(alignment: .bottom) {
                TabView(selection: $index) {
                    ForEach(Array(media.enumerated()), id: \.offset) { i, cid in
                        MediaTile(cid: cid)
                            .tag(i)
                            .onTapGesture { viewing = i }
                    }
                }
                .tabViewStyle(.page(indexDisplayMode: media.count > 1 ? .automatic : .never))
                .frame(height: 280)
                .clipShape(RoundedRectangle(cornerRadius: HeyRadius.attachment, style: .continuous))

                if media.count > 1 {
                    Text("\(index + 1)/\(media.count)")
                        .font(HeyFont.tick).foregroundStyle(.white)
                        .padding(.horizontal, 8).padding(.vertical, 3)
                        .background(.black.opacity(0.45), in: Capsule())
                        .padding(8)
                        .frame(maxWidth: .infinity, alignment: .trailing)
                }
            }
            .fullScreenCover(item: Binding(get: { viewing.map { Idx(id: $0) } }, set: { viewing = $0?.id })) { idx in
                FullImageViewer(media: media, start: idx.id)
            }
        }
    }
}

private struct Idx: Identifiable { let id: Int }

struct MediaTile: View {
    @Environment(\.colorScheme) private var scheme
    let cid: String
    var body: some View {
        // Resolved by namespace through the in-process content provider (hey-content://<cid>),
        // never by network. Shows the branded placeholder while loading / if absent.
        ContentImage(cid: cid) {
            ZStack {
                LinearGradient(colors: [Hey.bg2(scheme), Hey.bg3(scheme)], startPoint: .topLeading, endPoint: .bottomTrailing)
                Image(systemName: "photo").font(.system(size: 34)).foregroundStyle(Hey.muted(scheme))
            }
        }
        .scaledToFill()
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .clipped()
    }
}

/// Pinch-zoom + pan + page-swipe full-screen viewer.
struct FullImageViewer: View {
    @Environment(\.dismiss) private var dismiss
    let media: [String]
    @State var start: Int
    @State private var scale: CGFloat = 1
    @State private var offset: CGSize = .zero

    var body: some View {
        ZStack(alignment: .topTrailing) {
            Color.black.ignoresSafeArea()
            TabView(selection: $start) {
                ForEach(Array(media.enumerated()), id: \.offset) { i, cid in
                    MediaTile(cid: cid)
                        .scaleEffect(scale).offset(offset)
                        .gesture(MagnificationGesture().onChanged { scale = max(1, $0) }.onEnded { _ in if scale < 1.05 { withAnimation { scale = 1; offset = .zero } } })
                        .simultaneousGesture(DragGesture().onChanged { if scale > 1 { offset = $0.translation } })
                        .tag(i)
                }
            }
            .tabViewStyle(.page)
            Button { dismiss() } label: {
                Image(systemName: "xmark").font(.system(size: 15, weight: .bold)).foregroundStyle(.white)
                    .padding(12).background(.black.opacity(0.4), in: Circle())
            }
            .padding(.top, 12).padding(.trailing, 12)
        }
    }
}
