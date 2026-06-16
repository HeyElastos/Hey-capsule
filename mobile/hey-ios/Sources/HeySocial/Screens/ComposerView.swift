import SwiftUI
import PhotosUI
import UIKit
import UniformTypeIdentifiers

// 1:1 port of Android ComposerScreen (MainActivity.kt:1867-1971): a "Share a moment"
// sheet — pick up to 10 photos/videos, a polaroid-style stack preview with tap-✕ to
// remove, a caption field, and a gold "Share the moment" button. Each tile is
// uploaded via uploadMedia, then createPost publishes the post.
//
// onPosted is wired by the caller (FeedView) to refresh the feed.
struct ComposerView: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.dismiss) private var dismiss
    @Environment(\.colorScheme) private var scheme

    var onPosted: () -> Void = {}

    @State private var caption = ""
    @State private var picked: [PickedMedia] = []
    @State private var pickerItems: [PhotosPickerItem] = []
    @State private var busy = false
    @State private var status = ""

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                header
                Spacer().frame(height: 4)
                Text("Add a few photos or a video, then a caption.")
                    .font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))
                Spacer().frame(height: 16)

                if picked.isEmpty {
                    emptyPickerBox
                } else {
                    PolaroidStack(picked: picked, canAdd: picked.count < 10 && !busy) { i in
                        guard i < picked.count else { return }
                        picked.remove(at: i)
                    } onAdd: {
                        showPicker = true
                    }
                    Spacer().frame(height: 8)
                    Text("\(picked.count)/10 · tap ✕ to remove")
                        .font(.system(size: 12)).foregroundStyle(Hey.muted(scheme))
                        .frame(maxWidth: .infinity, alignment: .center)
                }

                Spacer().frame(height: 12)
                captionField
                Spacer().frame(height: 18)
                shareButton
                if !status.isEmpty {
                    Spacer().frame(height: 10)
                    Text(status).font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))
                }
                Spacer().frame(height: 20)
                Text("Pinned on-device · signed · federated via Carrier")
                    .font(HeyFont.timestamp).foregroundStyle(Hey.muted(scheme))
                    .frame(maxWidth: .infinity, alignment: .center)
            }
            .padding(.horizontal, 20).padding(.top, 16).padding(.bottom, 8)
            .animation(.easeInOut, value: picked.count)
        }
        .scrollContentBackground(.hidden)
        .background(Hey.sheetBg(scheme).ignoresSafeArea())
        .presentationDetents([.large])
        // The PhotosPicker is presented from the empty box / "Add" card via this binding.
        .photosPicker(isPresented: $showPicker, selection: $pickerItems,
                      maxSelectionCount: 10, matching: .any(of: [.images, .videos]))
        .onChange(of: pickerItems) { items in Task { await ingest(items) } }
        .interactiveDismissDisabled(busy)
    }

    @State private var showPicker = false

    // MARK: header
    private var header: some View {
        HStack(spacing: 8) {
            Image(systemName: "sparkles")
                .font(.system(size: 22)).foregroundStyle(Hey.goldInk(scheme))
            Text("Share a moment")
                .font(.system(size: 20, weight: .bold)).foregroundStyle(Hey.ink(scheme))
        }
    }

    // MARK: empty picker box (tap to pick — no separate button)
    private var emptyPickerBox: some View {
        Button { showPicker = true } label: {
            VStack(spacing: 0) {
                Image(systemName: "photo.badge.plus")
                    .font(.system(size: 44)).foregroundStyle(Hey.goldInk(scheme))
                Spacer().frame(height: 8)
                Text("Tap to add photos or video")
                    .font(.system(size: 15, weight: .semibold)).foregroundStyle(Hey.ink(scheme))
                Text("Up to 10 — they'll stack up here")
                    .font(.system(size: 12)).foregroundStyle(Hey.muted(scheme))
            }
            .frame(maxWidth: .infinity).frame(height: 170)
        }
        .buttonStyle(.plain)
        .glass(16)
        .disabled(busy)
    }

    private var captionField: some View {
        TextField("Write a caption…", text: $caption, axis: .vertical)
            .font(HeyFont.body).foregroundStyle(Hey.ink(scheme))
            .tint(Hey.gold)
            .padding(12)
            .background(Hey.glassFill(scheme), in: RoundedRectangle(cornerRadius: HeyRadius.attachment, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: HeyRadius.attachment, style: .continuous)
                    .strokeBorder(Hey.glassBorder(scheme), lineWidth: 1)
            )
    }

    private var shareButton: some View {
        Button {
            Task { await publish() }
        } label: {
            ZStack {
                if busy {
                    ProgressView().tint(Hey.navy)
                } else {
                    Text("Share the moment").font(.system(size: 15, weight: .bold))
                }
            }
            .frame(maxWidth: .infinity).frame(height: 52)
            .foregroundStyle(Hey.navy)
            .background(Hey.gold, in: RoundedRectangle(cornerRadius: HeyRadius.attachment, style: .continuous))
            .opacity(busy || picked.isEmpty ? 0.6 : 1)
        }
        .buttonStyle(.plain)
        .disabled(busy || picked.isEmpty)
    }

    // MARK: ingest picked items → PickedMedia
    private func ingest(_ items: [PhotosPickerItem]) async {
        guard !items.isEmpty else { return }
        status = "Reading…"
        let room = 10 - picked.count
        var added: [PickedMedia] = []
        for item in items.prefix(room) {
            guard let data = try? await item.loadTransferable(type: Data.self) else { continue }
            let isVideo = item.supportedContentTypes.contains { $0.conforms(to: .movie) }
            // Android shrinks images to WebP client-side; iOS sends the original bytes
            // (JPEG/PNG/HEIC) with their mime — the engine pins them as-is.
            // TODO: client-side downscale to match Android's WebP shrink.
            let mime = isVideo ? "video/mp4" : "image/jpeg"
            let preview = isVideo ? nil : UIImage(data: data)
            added.append(PickedMedia(data: data, mime: mime, isVideo: isVideo, preview: preview))
        }
        await MainActor.run {
            picked = Array((picked + added).prefix(10))
            pickerItems = []
            status = picked.isEmpty ? "Could not read those files" : "\(picked.count)/10 selected"
        }
    }

    // MARK: publish — upload each tile, then createPost
    private func publish() async {
        guard !picked.isEmpty else { status = "Add a photo or video first"; return }
        busy = true; status = "Publishing…"
        do {
            var tiles: [Media] = []
            for (i, pm) in picked.enumerated() {
                let fname = pm.isVideo ? "video\(i).mp4" : "photo\(i).jpg"
                let tile = try await store.engine.uploadMedia(pm.data, mime: pm.mime, name: fname)
                tiles.append(tile)
            }
            try await store.engine.createPost(caption: caption, tiles: tiles)
            busy = false
            onPosted()
            dismiss()
        } catch {
            busy = false
            status = "Failed: \(error.localizedDescription)"
        }
    }
}

private struct PickedMedia: Identifiable {
    let id = UUID()
    let data: Data
    let mime: String
    let isVideo: Bool
    let preview: UIImage?
}

// A playful fanned stack of polaroid-style cards for the chosen media — each card
// tilted, tap-✕ to remove, plus an "Add" card at the end (Android PolaroidStack,
// MainActivity.kt:1977-2024).
private struct PolaroidStack: View {
    @Environment(\.colorScheme) private var scheme
    let picked: [PickedMedia]
    var canAdd: Bool
    var onRemove: (Int) -> Void
    var onAdd: () -> Void

    private let tilts: [Double] = [-5, 4, -3, 5, -2, 3]

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: -16) {
                ForEach(Array(picked.enumerated()), id: \.element.id) { i, pm in
                    ZStack(alignment: .topLeading) {
                        card(pm)
                        Button { onRemove(i) } label: {
                            Image(systemName: "xmark")
                                .font(.system(size: 14, weight: .bold)).foregroundStyle(.white)
                                .frame(width: 24, height: 24)
                                .background(Hey.navy.opacity(0.9), in: Circle())
                        }
                        .buttonStyle(.plain)
                        .offset(x: -3, y: -3)
                    }
                    .rotationEffect(.degrees(tilts[i % tilts.count]))
                }
                if canAdd { addCard }
            }
            .padding(.horizontal, 10).padding(.top, 16).padding(.bottom, 8)
        }
        .frame(height: 184)
    }

    private var addCard: some View {
        Button { onAdd() } label: {
            VStack(spacing: 4) {
                Image(systemName: "photo.badge.plus")
                    .font(.system(size: 34)).foregroundStyle(Hey.goldInk(scheme))
                Text("Add").font(.system(size: 12)).foregroundStyle(Hey.muted(scheme))
            }
            .frame(width: 118, height: 118)
            .padding(EdgeInsets(top: 7, leading: 7, bottom: 16, trailing: 7))
            .background(Color.white.opacity(0.12))
            .overlay(
                RoundedRectangle(cornerRadius: HeyRadius.attachment, style: .continuous)
                    .strokeBorder(Hey.glassBorder(scheme), lineWidth: 1.5)
            )
            .clipShape(RoundedRectangle(cornerRadius: HeyRadius.attachment, style: .continuous))
        }
        .buttonStyle(.plain)
        .rotationEffect(.degrees(3))
    }

    // Polaroid frame: white card, photo, extra bottom lip.
    private func card(_ pm: PickedMedia) -> some View {
        VStack(spacing: 0) {
            ZStack {
                Color(hex: 0xE9E9EE)
                if let preview = pm.preview {
                    Image(uiImage: preview).resizable().scaledToFill()
                } else {
                    Image(systemName: "play.circle.fill").font(.system(size: 42)).foregroundStyle(Hey.navy)
                }
            }
            .frame(width: 118, height: 118)
            .clipShape(RoundedRectangle(cornerRadius: HeyRadius.thumb, style: .continuous))
        }
        .padding(EdgeInsets(top: 7, leading: 7, bottom: 16, trailing: 7))
        .background(Color.white)
        .clipShape(RoundedRectangle(cornerRadius: HeyRadius.attachment, style: .continuous))
        .shadow(color: .black.opacity(0.2), radius: 8, y: 3)
    }
}
