import SwiftUI
import PhotosUI
import UIKit

/// Edit my profile — nickname + bio + avatar (port of EditProfileSheet,
/// MainActivity.kt:2663-2717). Picks an image via PhotosUI, downscales it,
/// uploads it (engine.uploadMedia), then saves via engine.saveProfile.
/// Presented from ProfileView as a bottom sheet; calls `onSaved` on success.
struct EditProfileSheet: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var scheme
    @Environment(\.dismiss) private var dismiss

    let current: Profile
    var onSaved: () -> Void

    @State private var nickname: String
    @State private var bio: String
    @State private var avatarCid: String
    @State private var pickedItem: PhotosPickerItem?
    @State private var pickedImage: UIImage?
    @State private var avatarBytes: Data?
    @State private var busy = false

    init(current: Profile, onSaved: @escaping () -> Void) {
        self.current = current
        self.onSaved = onSaved
        _nickname = State(initialValue: current.nickname)
        _bio = State(initialValue: current.bio)
        _avatarCid = State(initialValue: current.avatar)
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 16) {
                    Text("Edit profile")
                        .font(HeyFont.subtitle.weight(.bold))
                        .foregroundStyle(Hey.ink(scheme))
                        .padding(.top, 4)

                    PhotosPicker(selection: $pickedItem, matching: .images) {
                        ZStack {
                            Circle().fill(Hey.avatarGradient)
                            if let pickedImage {
                                Image(uiImage: pickedImage)
                                    .resizable().scaledToFill()
                                    .frame(width: 88, height: 88)
                                    .clipShape(Circle())
                            } else if !avatarCid.isEmpty {
                                ContentImage(cid: avatarCid) {
                                    Image(systemName: "camera.fill")
                                        .font(.system(size: 28))
                                        .foregroundStyle(Hey.navy)
                                }
                                .scaledToFill()
                                .frame(width: 88, height: 88)
                                .clipShape(Circle())
                            } else {
                                Image(systemName: "camera.fill")
                                    .font(.system(size: 28))
                                    .foregroundStyle(Hey.navy)
                            }
                        }
                        .frame(width: 88, height: 88)
                    }

                    field(placeholder: "Nickname", text: $nickname, axis: .horizontal)
                    field(placeholder: "Bio", text: $bio, axis: .vertical)

                    Button {
                        save()
                    } label: {
                        Group {
                            if busy {
                                ProgressView().tint(Hey.navy)
                            } else {
                                Text("Save").font(HeyFont.label.weight(.bold))
                            }
                        }
                        .frame(maxWidth: .infinity, minHeight: 50)
                    }
                    .foregroundStyle(Hey.navy)
                    .background(Hey.gold, in: RoundedRectangle(cornerRadius: HeyRadius.attachment, style: .continuous))
                    .disabled(busy)
                    .padding(.top, 2)

                    Spacer(minLength: 8)
                }
                .padding(20)
            }
            .scrollContentBackground(.hidden)
            .background(Hey.sheetBg(scheme).ignoresSafeArea())
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }.tint(Hey.muted(scheme))
                }
            }
        }
        .presentationDetents([.medium, .large])
        .onChange(of: pickedItem) { item in
            guard let item else { return }
            Task { await loadPicked(item) }
        }
    }

    @ViewBuilder
    private func field(placeholder: String, text: Binding<String>, axis: Axis) -> some View {
        TextField(placeholder, text: text, axis: axis)
            .font(HeyFont.body)
            .foregroundStyle(Hey.ink(scheme))
            .tint(Hey.gold)
            .padding(14)
            .frame(maxWidth: .infinity, alignment: .leading)
            .glass(HeyRadius.attachment)
    }

    private func loadPicked(_ item: PhotosPickerItem) async {
        guard let data = try? await item.loadTransferable(type: Data.self),
              let img = UIImage(data: data) else { return }
        // Downscale to a 512px avatar + JPEG-encode (Android: scaleWebp 512/82;
        // iOS canvas can't encode WebP, JPEG is the faithful equivalent).
        let scaled = img.downscaled(maxEdge: 512)
        let bytes = scaled.jpegData(compressionQuality: 0.82)
        await MainActor.run {
            self.pickedImage = scaled
            self.avatarBytes = bytes
        }
    }

    private func save() {
        busy = true
        Task {
            do {
                if let bytes = avatarBytes {
                    let media = try await store.engine.uploadMedia(bytes, mime: "image/jpeg", name: "avatar.jpg")
                    avatarCid = media.cid
                }
                let name = nickname.trimmingCharacters(in: .whitespacesAndNewlines)
                try await store.engine.saveProfile(
                    nickname: name.isEmpty ? "Hey user" : name,
                    bio: bio.trimmingCharacters(in: .whitespacesAndNewlines),
                    avatarCid: avatarCid
                )
                await MainActor.run { busy = false; onSaved(); dismiss() }
            } catch {
                await MainActor.run { busy = false }
            }
        }
    }
}

private extension UIImage {
    /// Aspect-fit downscale so the longest edge is `maxEdge` (no upscaling).
    func downscaled(maxEdge: CGFloat) -> UIImage {
        let longest = max(size.width, size.height)
        guard longest > maxEdge, longest > 0 else { return self }
        let scale = maxEdge / longest
        let target = CGSize(width: size.width * scale, height: size.height * scale)
        let format = UIGraphicsImageRendererFormat.default()
        format.scale = 1
        return UIGraphicsImageRenderer(size: target, format: format).image { _ in
            draw(in: CGRect(origin: .zero, size: target))
        }
    }
}
