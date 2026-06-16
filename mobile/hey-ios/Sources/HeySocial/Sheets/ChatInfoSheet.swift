import SwiftUI
import UIKit

// Contact / group info sheet — 1:1 port of ChatInfoSheet (MainActivity.kt:4671-4765).
// Frosted modal: avatar + name + "end-to-end encrypted", then a stack of actions
// (View profile / Voice call / Send a gift / Mute / Block & remove) and a grid of
// shared photos pulled from the conversation's image attachments.
//
// Engine note: Android reads mute/block from LOCAL prefs (HeyApi.isChatMuted /
// setChatMuted / setBlocked) — these are device-local toggles, NOT engine methods.
// iOS mirrors that with an App-Group UserDefaults store so the toggle is faithful
// without inventing a contract method. `delete` is real (engine.deleteChat).
struct ChatInfoSheet: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var scheme

    let chat: Chat
    /// The conversation, so we can surface shared photos (Android passes `msgs`).
    /// Empty is fine — the "Shared photos" section just hides.
    var messages: [Message] = []
    var onViewProfile: () -> Void = {}
    var onCall: () -> Void = {}
    var onTip: () -> Void = {}
    var onDelete: () -> Void = {}      // "Block & remove" / delete chat
    var onClose: () -> Void = {}

    @State private var isMuted = false
    @State private var viewer: ChatSharedPhoto? = nil

    private var photoAttachments: [Attachment] {
        messages.flatMap { $0.attachments }.filter { $0.isImage }
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                // ── Header: avatar + name + encryption badge ──
                HStack(spacing: 14) {
                    Avatar(name: chat.name, size: 56, online: chat.isGroup ? nil : chat.online, cid: chat.avatar)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(chat.name)
                            .font(.system(size: 20, weight: .bold))
                            .foregroundStyle(Hey.ink(scheme))
                        HStack(spacing: 3) {
                            Image(systemName: "lock.fill")
                                .font(.system(size: 11))
                                .foregroundStyle(Hey.good(scheme))
                            Text("end-to-end encrypted")
                                .font(.system(size: 12))
                                .foregroundStyle(Hey.muted(scheme))
                        }
                    }
                    Spacer(minLength: 0)
                }
                .padding(.bottom, 16)

                // ── Actions ──
                ChatInfoAction(icon: "person.fill", label: "View profile") {
                    onClose(); onViewProfile()
                }
                ChatInfoAction(icon: "phone.fill", label: "Voice call") {
                    onClose(); onCall()
                }
                ChatInfoAction(icon: "dollarsign.circle.fill", label: "Send a gift / tip") {
                    onClose(); onTip()
                }

                // Mute toggle (local pref)
                HStack(spacing: 14) {
                    Image(systemName: isMuted ? "bell.slash.fill" : "bell.fill")
                        .foregroundStyle(Hey.goldInk(scheme))
                        .frame(width: 24)
                    Text("Mute notifications")
                        .font(.system(size: 15))
                        .foregroundStyle(Hey.ink(scheme))
                    Spacer()
                    Toggle("", isOn: $isMuted)
                        .labelsHidden()
                        .tint(Hey.gold)
                }
                .padding(12)
                .contentShape(Rectangle())
                .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
                .onChange(of: isMuted) { ChatPrefs.setMuted(chat.id, $0) }

                ChatInfoAction(icon: "nosign", label: "Block & remove", danger: true) {
                    ChatPrefs.setBlocked(chat.id, true)
                    Task { await store.engine.deleteChat(chat) }
                    onClose(); onDelete()
                }

                // ── Shared photos ──
                if !photoAttachments.isEmpty {
                    Text("Shared photos · \(photoAttachments.count)")
                        .font(.system(size: 14, weight: .semibold))
                        .foregroundStyle(Hey.ink(scheme))
                        .padding(.top, 18).padding(.bottom, 10)

                    let rows = stride(from: 0, to: photoAttachments.count, by: 3).map {
                        Array(photoAttachments[$0..<min($0 + 3, photoAttachments.count)])
                    }
                    VStack(spacing: 6) {
                        ForEach(Array(rows.enumerated()), id: \.offset) { _, row in
                            HStack(spacing: 6) {
                                ForEach(Array(row.enumerated()), id: \.offset) { _, att in
                                    SharedPhotoTile(att: att) { bytes in
                                        viewer = ChatSharedPhoto(name: att.name, data: bytes)
                                    }
                                }
                                ForEach(0..<(3 - row.count), id: \.self) { _ in
                                    Color.clear.frame(maxWidth: .infinity).aspectRatio(1, contentMode: .fit)
                                }
                            }
                        }
                    }
                }
            }
            .padding(20)
            .padding(.bottom, 20)
        }
        .scrollContentBackground(.hidden)
        .background(Hey.sheetBg(scheme).ignoresSafeArea())
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
        .task { isMuted = ChatPrefs.isMuted(chat.id) }
        .fullScreenCover(item: $viewer) { p in
            SharedPhotoViewer(photo: p) { viewer = nil }
        }
    }
}

// One tappable row in the action stack (ChatInfoAction, MainActivity.kt:4739-4749).
private struct ChatInfoAction: View {
    @Environment(\.colorScheme) private var scheme
    let icon: String
    let label: String
    var danger: Bool = false
    let onClick: () -> Void

    var body: some View {
        Button(action: onClick) {
            HStack(spacing: 14) {
                Image(systemName: icon)
                    .foregroundStyle(danger ? Hey.like : Hey.goldInk(scheme))
                    .frame(width: 24)
                Text(label)
                    .font(.system(size: 15))
                    .foregroundStyle(danger ? Hey.like : Hey.ink(scheme))
                Spacer(minLength: 0)
            }
            .padding(12)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
    }
}

// A 1:1 square thumbnail loaded from the attachment bytes (SharedPhoto, MainActivity.kt:4751).
private struct SharedPhotoTile: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var scheme
    let att: Attachment
    let onOpen: (Data) -> Void

    @State private var bytes: Data? = nil

    var body: some View {
        Button {
            if let bytes { onOpen(bytes) }
        } label: {
            ZStack {
                Hey.glassFill(scheme)
                if let bytes, let img = UIImage(data: bytes) {
                    Image(uiImage: img).resizable().scaledToFill()
                } else {
                    ProgressView().tint(Hey.goldInk(scheme))
                }
            }
            .frame(maxWidth: .infinity)
            .aspectRatio(1, contentMode: .fit)
            .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
        }
        .buttonStyle(.plain)
        .disabled(bytes == nil)
        .task {
            let data = await store.engine.fetchAttachment(att)
            if let data, !data.isEmpty { bytes = data }
        }
    }
}

// Full-screen viewer for a shared photo we already have bytes for.
private struct ChatSharedPhoto: Identifiable {
    let id = UUID()
    let name: String
    let data: Data
}

private struct SharedPhotoViewer: View {
    let photo: ChatSharedPhoto
    let onClose: () -> Void

    var body: some View {
        ZStack(alignment: .topTrailing) {
            Color.black.ignoresSafeArea()
            if let img = UIImage(data: photo.data) {
                Image(uiImage: img).resizable().scaledToFit().ignoresSafeArea()
            }
            Button(action: onClose) {
                Image(systemName: "xmark")
                    .font(.system(size: 15, weight: .bold))
                    .foregroundStyle(.white)
                    .padding(12)
                    .background(.black.opacity(0.4), in: Circle())
            }
            .padding(.top, 12).padding(.trailing, 12)
        }
    }
}

// Device-local chat preferences (mute / block). Mirrors Android's HeyApi prefs,
// which are NOT engine methods — they live in shared storage so the app + the
// notification extension agree on what's muted. Stored in the App-Group suite.
enum ChatPrefs {
    private static let store = UserDefaults(suiteName: AppPaths.appGroup) ?? .standard
    private static func mutedKey(_ id: String) -> String { "chat.muted.\(id)" }
    private static func blockedKey(_ id: String) -> String { "chat.blocked.\(id)" }

    static func isMuted(_ id: String) -> Bool { store.bool(forKey: mutedKey(id)) }
    static func setMuted(_ id: String, _ on: Bool) { store.set(on, forKey: mutedKey(id)) }
    static func isBlocked(_ id: String) -> Bool { store.bool(forKey: blockedKey(id)) }
    static func setBlocked(_ id: String, _ on: Bool) { store.set(on, forKey: blockedKey(id)) }
}
