import SwiftUI
import PhotosUI
import UniformTypeIdentifiers

// Conversation — port of ConversationScreen (MainActivity.kt:4766-5138).
// • Floating glass header: back · avatar/name + E2E-encrypted line · call · search.
//   Tapping the avatar/name (1:1) → onOpenInfo. Call button → onCall.
// • Messages scroll the full height; bubbles: mine = solid Gold + Navy text,
//   theirs = bubbleIn + ink; group rows show the sender name.
// • Hold YOUR message → Edit / Delete sheet; hold a RECEIVED message → reaction
//   picker. Reaction chips under the bubble toggle yours.
// • Attachments: images inline (fetchAttachment → full viewer), files as a row.
// • Composer: attach (PhotosUI + files) stages items in a tray, optional caption,
//   then Send (send + sendAttachment). A transfer bar shows while uploading.
// • Polls the conversation every 1.5s; marks 1:1 read on open.
//
// Navigation is NOT ours: onBack / onOpenInfo / onCall / onOpenProfile are closures
// wired by the orchestrator.
struct ChatDetailView: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var scheme

    let chat: Chat
    var onBack: () -> Void = {}
    var onOpenInfo: () -> Void = {}
    var onCall: () -> Void = {}
    var onOpenProfile: (String) -> Void = { _ in }

    @State private var msgs: [Message] = []
    @State private var reactions: [String: [MsgReaction]] = [:]
    @State private var input = ""
    @State private var query: String?            // nil = search closed
    @State private var sending = false
    @State private var transferLabel: String?
    @State private var staged: [StagedItem] = []
    @State private var reactTarget: String?       // received message id → emoji picker
    @State private var actionTarget: Message?     // own message → edit/delete sheet
    @State private var editTarget: Message?
    @State private var editText = ""
    @State private var deleteTarget: String?
    @State private var photoItems: [PhotosPickerItem] = []
    @State private var presentPhotos = false
    @State private var showFiles = false

    private var shown: [Message] {
        guard let q = query, !q.trimmingCharacters(in: .whitespaces).isEmpty else { return msgs }
        return msgs.filter { $0.text.range(of: q, options: .caseInsensitive) != nil }
    }

    var body: some View {
        ZStack {
            FrostBackground().ignoresSafeArea()

            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(spacing: 0) {
                        Color.clear.frame(height: 66)   // clears the floating header
                        ForEach(shown) { m in
                            Bubble(msg: m, isGroup: chat.isGroup,
                                   reactions: reactions[m.id] ?? [],
                                   onLongPress: { if m.mine { actionTarget = m } else { reactTarget = m.id } },
                                   onReact: { react(m.id, $0) },
                                   fetch: { await store.engine.fetchAttachment($0) })
                                .id(m.id)
                        }
                        Color.clear.frame(height: 90)   // clears the floating composer
                    }
                    .padding(.horizontal, 12)
                }
                .scrollContentBackground(.hidden)
                .onChange(of: msgs.count) { _ in
                    if let last = msgs.last { withAnimation { proxy.scrollTo(last.id, anchor: .bottom) } }
                }
            }

            VStack { header; Spacer() }
            VStack { Spacer(); composer }
        }
        .navigationBarHidden(true)
        .photosPicker(isPresented: photoBinding, selection: $photoItems, maxSelectionCount: 10, matching: .any(of: [.images, .videos]))
        .onChange(of: photoItems) { _ in Task { await stagePhotos() } }
        .fileImporter(isPresented: $showFiles, allowedContentTypes: [.item], allowsMultipleSelection: true) { result in
            stageFiles(result)
        }
        .confirmationDialog("Message", isPresented: actionBinding, titleVisibility: .visible) {
            Button("Edit") { if let m = actionTarget { editText = m.text; editTarget = m }; actionTarget = nil }
            Button("Delete", role: .destructive) { if let m = actionTarget { deleteTarget = m.id }; actionTarget = nil }
            Button("Cancel", role: .cancel) { actionTarget = nil }
        }
        .confirmationDialog("Delete message?", isPresented: deleteBinding, titleVisibility: .visible) {
            Button("Delete", role: .destructive) { if let id = deleteTarget { delete(id) }; deleteTarget = nil }
            Button("Cancel", role: .cancel) { deleteTarget = nil }
        } message: {
            Text("Removed for everyone in this chat.")
        }
        .sheet(item: $editTarget) { m in editSheet(m) }
        .overlay { if reactTarget != nil { reactionPicker } }
        .task(id: chat.id) { await openAndPoll() }
    }

    // ── floating glass header (MainActivity.kt:4884-4926) ──
    private var header: some View {
        HStack(spacing: 4) {
            Button { onBack() } label: {
                Image(systemName: "chevron.backward").font(.system(size: 17, weight: .semibold)).foregroundStyle(Hey.ink(scheme))
                    .frame(width: 40, height: 40)
            }
            if query != nil {
                TextField("Search messages…", text: Binding(get: { query ?? "" }, set: { query = $0 }))
                    .font(HeyFont.body).foregroundStyle(Hey.ink(scheme))
                    .textFieldStyle(.plain)
                    .frame(maxWidth: .infinity)
                Button { query = nil } label: {
                    Image(systemName: "xmark").font(.system(size: 15, weight: .semibold)).foregroundStyle(Hey.ink(scheme))
                        .frame(width: 40, height: 40)
                }
            } else {
                headerIdentity
                Button { onCall() } label: { Image(systemName: "phone.fill") }
                    .foregroundStyle(Hey.goldInk(scheme)).frame(width: 40, height: 40)
                Button { query = "" } label: { Image(systemName: "magnifyingglass") }
                    .foregroundStyle(Hey.muted(scheme)).frame(width: 40, height: 40)
            }
        }
        .padding(.horizontal, 4).padding(.vertical, 2)
        .background(headerGlass)
        .padding(.horizontal, 10).padding(.top, 8)
    }

    private var headerIdentity: some View {
        Button {
            if !chat.isGroup { onOpenInfo() }
        } label: {
            HStack(spacing: 10) {
                if chat.isGroup {
                    Circle().fill(Hey.avatarGradient)
                        .frame(width: 40, height: 40)
                        .overlay(Image(systemName: "person.3.fill").font(.system(size: 16)).foregroundStyle(Hey.navy))
                } else {
                    Avatar(name: chat.name, size: 40, cid: chat.avatar)
                }
                VStack(alignment: .leading, spacing: 1) {
                    Text(chat.name)
                        .font(.system(size: 17, weight: .semibold)).foregroundStyle(Hey.ink(scheme))
                        .lineLimit(1)
                    HStack(spacing: 3) {
                        Image(systemName: "lock.fill").font(.system(size: 11)).foregroundStyle(Hey.good(scheme))
                        Text(chat.isGroup ? "group · end-to-end encrypted" : "end-to-end encrypted")
                            .font(HeyFont.timestamp).foregroundStyle(Hey.muted(scheme))
                            .lineLimit(1)
                    }
                }
                Spacer(minLength: 0)
            }
        }
        .buttonStyle(.plain)
        .disabled(chat.isGroup)
    }

    private var headerGlass: some View {
        RoundedRectangle(cornerRadius: 24, style: .continuous)
            .fill(Hey.bg2(scheme).opacity(0.84))
            .overlay(RoundedRectangle(cornerRadius: 24, style: .continuous).fill(.ultraThinMaterial).opacity(0.5))
            .overlay(RoundedRectangle(cornerRadius: 24, style: .continuous).strokeBorder(Hey.glassBorder(scheme), lineWidth: 1))
    }

    // ── floating glass composer (MainActivity.kt:4954-5031) ──
    private var composer: some View {
        VStack(spacing: 0) {
            // Staged attachments tray.
            if !staged.isEmpty && !sending {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 8) {
                        ForEach(staged) { item in stagedThumb(item) }
                    }
                    .padding(.vertical, 6)
                }
            }
            // Transfer bar while uploading.
            if sending {
                VStack(alignment: .leading, spacing: 3) {
                    Text(transferLabel ?? "Sending…").font(HeyFont.timestamp).foregroundStyle(Hey.muted(scheme))
                    ProgressView().progressViewStyle(.linear).tint(Hey.goldInk(scheme))
                }
                .padding(.horizontal, 6).padding(.vertical, 2)
            }
            // Input bar — one floating glass panel: attach · text · send.
            HStack(spacing: 4) {
                Menu {
                    Button { showFiles = true } label: { Label("File", systemImage: "doc") }
                    Button { presentPhotos = true } label: { Label("Photo or Video", systemImage: "photo") }
                } label: {
                    if sending {
                        ProgressView().tint(Hey.goldInk(scheme)).frame(width: 44, height: 44)
                    } else {
                        Image(systemName: "paperclip").foregroundStyle(Hey.muted(scheme)).frame(width: 44, height: 44)
                    }
                }
                .disabled(sending)

                TextField("Message…", text: $input, axis: .vertical)
                    .font(HeyFont.body).foregroundStyle(Hey.ink(scheme))
                    .tint(Hey.gold)
                    .lineLimit(1...5)
                    .padding(.vertical, 12).padding(.trailing, 8)

                Button { sendStaged() } label: {
                    Image(systemName: "paperplane.fill")
                        .font(.system(size: 16, weight: .semibold)).foregroundStyle(Hey.navy)
                        .frame(width: 42, height: 42)
                        .background(canSend ? Hey.gold : Hey.gold.opacity(0.72), in: Circle())
                }
                .disabled(!canSend || sending)
            }
            .padding(.horizontal, 4).padding(.vertical, 4)
            .background(headerGlass.clipShape(RoundedRectangle(cornerRadius: 28, style: .continuous)))
        }
        .padding(.horizontal, 10).padding(.vertical, 8)
    }

    private var photoBinding: Binding<Bool> {
        Binding(get: { presentPhotos }, set: { presentPhotos = $0 })
    }
    private var actionBinding: Binding<Bool> {
        Binding(get: { actionTarget != nil }, set: { if !$0 { actionTarget = nil } })
    }
    private var deleteBinding: Binding<Bool> {
        Binding(get: { deleteTarget != nil }, set: { if !$0 { deleteTarget = nil } })
    }
    private var canSend: Bool {
        !input.trimmingCharacters(in: .whitespaces).isEmpty || !staged.isEmpty
    }

    private func stagedThumb(_ item: StagedItem) -> some View {
        ZStack(alignment: .topTrailing) {
            ZStack {
                RoundedRectangle(cornerRadius: 10, style: .continuous).fill(Hey.glassFill(scheme))
                if item.isImage, let ui = UIImage(data: item.data) {
                    Image(uiImage: ui).resizable().scaledToFill()
                        .frame(width: 64, height: 64)
                        .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
                } else {
                    Image(systemName: "doc.fill").font(.system(size: 28)).foregroundStyle(Hey.goldInk(scheme))
                }
            }
            .frame(width: 64, height: 64)
            Button { staged.removeAll { $0.id == item.id } } label: {
                Image(systemName: "xmark").font(.system(size: 11, weight: .bold)).foregroundStyle(.white)
                    .frame(width: 20, height: 20).background(Color.black.opacity(0.8), in: Circle())
            }
            .padding(2)
        }
        .frame(width: 64, height: 64)
    }

    // ── emoji reaction picker (MainActivity.kt:5044-5059) ──
    private var reactionPicker: some View {
        ZStack {
            Color.black.opacity(0.001).ignoresSafeArea().onTapGesture { reactTarget = nil }
            HStack(spacing: 2) {
                ForEach(["👍", "❤️", "😂", "😮", "😢", "🎉", "🙏", "🔥"], id: \.self) { e in
                    Text(e).font(.system(size: 26))
                        .padding(6)
                        .onTapGesture {
                            if let id = reactTarget { react(id, e) }
                            reactTarget = nil
                        }
                }
            }
            .padding(.horizontal, 10).padding(.vertical, 8)
            .glass(22)
        }
    }

    // ── edit-in-place sheet (MainActivity.kt:5091-5118) ──
    private func editSheet(_ m: Message) -> some View {
        NavigationStack {
            ZStack {
                FrostBackground().ignoresSafeArea()
                VStack {
                    TextField("Edit message", text: $editText, axis: .vertical)
                        .font(HeyFont.body).foregroundStyle(Hey.ink(scheme))
                        .padding(12)
                        .background(Hey.glassFill(scheme), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                        .padding()
                    Spacer()
                }
            }
            .navigationTitle("Edit message").navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { editTarget = nil } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { saveEdit(m) }
                        .fontWeight(.bold)
                        .disabled(editText.trimmingCharacters(in: .whitespaces).isEmpty || editText == m.text)
                }
            }
        }
        .presentationDetents([.height(220)])
        .tint(Hey.goldInk(scheme))
    }

    // ── data ──
    private func openAndPoll() async {
        if !chat.isGroup { await store.engine.markRead(did: chat.id) }
        while !Task.isCancelled {
            await reload()
            try? await Task.sleep(nanoseconds: 1_500_000_000)
        }
    }

    private func reload() async {
        if let m = try? await store.engine.conversation(chat), m != msgs { msgs = m }
        let r = await store.engine.messageReactions(chat)
        if r != reactions { reactions = r }
    }

    private func sendStaged() {
        guard !sending else { return }
        let text = input.trimmingCharacters(in: .whitespaces)
        let items = staged
        guard !text.isEmpty || !items.isEmpty else { return }
        input = ""; staged = []
        sending = true
        transferLabel = items.isEmpty ? nil : "Sending \(items.count) \(items.count == 1 ? "item" : "items")…"
        Task {
            if !text.isEmpty { try? await store.engine.send(chat, text: text) }
            for it in items {
                try? await store.engine.sendAttachment(chat, data: it.data, mime: it.mime, name: it.name, text: "")
            }
            transferLabel = nil
            sending = false
            await reload()
        }
    }

    private func react(_ id: String, _ emoji: String) {
        Task {
            try? await store.engine.reactToMessage(chat, msgId: id, emoji: emoji)
            await reload()
        }
    }

    private func delete(_ id: String) {
        Task {
            try? await store.engine.deleteMessage(chat, msgId: id)
            await reload()
        }
    }

    private func saveEdit(_ m: Message) {
        let t = editText.trimmingCharacters(in: .whitespaces)
        editTarget = nil
        guard !t.isEmpty, t != m.text else { return }
        Task {
            try? await store.engine.editMessage(chat, msgId: m.id, text: t)
            await reload()
        }
    }

    // Stage photos/videos picked via PhotosUI (cap a batch at 10).
    private func stagePhotos() async {
        var add: [StagedItem] = []
        for item in photoItems {
            guard let data = try? await item.loadTransferable(type: Data.self) else { continue }
            let isImage = item.supportedContentTypes.contains { $0.conforms(to: .image) && $0 != .gif }
            let mime = item.supportedContentTypes.first?.preferredMIMEType
                ?? (isImage ? "image/jpeg" : "application/octet-stream")
            let ext = item.supportedContentTypes.first?.preferredFilenameExtension ?? (isImage ? "jpg" : "bin")
            add.append(StagedItem(data: data, mime: mime, name: "photo.\(ext)", isImage: isImage))
        }
        photoItems = []
        if !add.isEmpty { staged = Array((staged + add).prefix(10)) }
    }

    // Stage files picked via the document importer.
    private func stageFiles(_ result: Result<[URL], Error>) {
        guard case .success(let urls) = result else { return }
        var add: [StagedItem] = []
        for url in urls {
            let needsStop = url.startAccessingSecurityScopedResource()
            defer { if needsStop { url.stopAccessingSecurityScopedResource() } }
            guard let data = try? Data(contentsOf: url) else { continue }
            let mime = UTType(filenameExtension: url.pathExtension)?.preferredMIMEType ?? "application/octet-stream"
            add.append(StagedItem(data: data, mime: mime, name: url.lastPathComponent,
                                  isImage: mime.hasPrefix("image/") && mime != "image/gif"))
        }
        if !add.isEmpty { staged = Array((staged + add).prefix(10)) }
    }
}

/// A file/photo the user picked but hasn't sent yet (staged in the composer tray) —
/// port of StagedItem (MainActivity.kt:4626). Holds the resolved bytes so iOS doesn't
/// need a security-scoped URL at send time.
private struct StagedItem: Identifiable {
    let id = UUID()
    let data: Data
    let mime: String
    let name: String
    let isImage: Bool
}

// One message bubble — port of Bubble (MainActivity.kt:5140-5187).
// mine = solid Gold + Navy text (tail bottom-right); theirs = bubbleIn + ink
// (tail bottom-left). Group rows show the sender name above an incoming bubble.
// Hold → onLongPress (own = action sheet, received = reaction picker). Reaction
// chips toggle yours. Time + double-tick sit inside the bubble, bottom-right.
private struct Bubble: View {
    @Environment(\.colorScheme) private var scheme
    let msg: Message
    let isGroup: Bool
    let reactions: [MsgReaction]
    var onLongPress: () -> Void = {}
    var onReact: (String) -> Void = { _ in }
    var fetch: (Attachment) async -> Data? = { _ in nil }

    private var attachmentOnly: Bool { !msg.attachments.isEmpty && msg.text.isEmpty }

    var body: some View {
        VStack(alignment: msg.mine ? .trailing : .leading, spacing: 3) {
            if isGroup && !msg.mine && !msg.sender.isEmpty {
                Text(msg.sender).font(HeyFont.timestamp).foregroundStyle(Hey.goldInk(scheme))
                    .padding(.leading, 6).padding(.bottom, 1)
            }
            VStack(alignment: .leading, spacing: 0) {
                ForEach(Array(msg.attachments.enumerated()), id: \.offset) { _, att in
                    AttachmentView(att: att, mine: msg.mine, fetch: fetch)
                }
                if !msg.text.isEmpty {
                    if !msg.attachments.isEmpty { Spacer().frame(height: 6) }
                    Text(msg.text).font(HeyFont.body).foregroundStyle(msg.mine ? Hey.navy : Hey.ink(scheme))
                }
                if msg.ts > 0 {
                    HStack(spacing: 3) {
                        Text(RelativeTime.clock(msg.ts)).font(HeyFont.tick)
                            .foregroundStyle(msg.mine ? Hey.navy.opacity(0.6) : Hey.muted(scheme))
                        if msg.mine {
                            Image(systemName: "checkmark.circle.fill").font(.system(size: 10))
                                .foregroundStyle(Hey.navy.opacity(0.6))
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .trailing)
                    .padding(.top, 3)
                }
            }
            .padding(.horizontal, attachmentOnly ? 6 : 12)
            .padding(.vertical, attachmentOnly ? 6 : 8)
            .frame(maxWidth: 300, alignment: .leading)
            .fixedSize(horizontal: false, vertical: true)
            .background(bubbleBackground)
            .clipShape(bubbleShape)
            .overlay {
                if !msg.mine && scheme == .light {
                    bubbleShape.strokeBorder(Hey.glassBorder(scheme), lineWidth: 1)
                }
            }
            .contentShape(Rectangle())
            .onLongPressGesture { onLongPress() }

            if !reactions.isEmpty { reactionChips }
        }
        .frame(maxWidth: .infinity, alignment: msg.mine ? .trailing : .leading)
        .padding(.vertical, 3)
    }

    private var bubbleShape: RoundedCornerShape {
        msg.mine
        ? RoundedCornerShape(tl: 18, tr: 18, bl: 18, br: 4)
        : RoundedCornerShape(tl: 18, tr: 18, bl: 4, br: 18)
    }
    private var bubbleBackground: some ShapeStyle {
        msg.mine ? AnyShapeStyle(Hey.gold) : AnyShapeStyle(Hey.bubbleIn(scheme))
    }

    private var reactionChips: some View {
        let grouped = Dictionary(grouping: reactions, by: { $0.emoji })
        return HStack(spacing: 4) {
            ForEach(grouped.keys.sorted(), id: \.self) { emoji in
                Text("\(emoji) \(grouped[emoji]?.count ?? 0)")
                    .font(.system(size: 12)).foregroundStyle(Hey.ink(scheme))
                    .padding(.horizontal, 7).padding(.vertical, 2)
                    .background(
                        RoundedRectangle(cornerRadius: HeyRadius.reaction, style: .continuous)
                            .fill(Hey.glassFill(scheme))
                            .overlay(RoundedRectangle(cornerRadius: HeyRadius.reaction, style: .continuous)
                                .strokeBorder(Hey.glassBorder(scheme), lineWidth: 1))
                    )
                    .onTapGesture { onReact(emoji) }
            }
        }
        .padding(.top, 3)
    }
}

// One attachment — port of AttachmentView (MainActivity.kt:5191-5240).
// Images load inline (fetchAttachment → decode); tap opens a full-screen viewer.
// On failure, a "Tap to load photo" retry tile. Non-images render as an icon + name
// + human size row.
private struct AttachmentView: View {
    @Environment(\.colorScheme) private var scheme
    let att: Attachment
    let mine: Bool
    var fetch: (Attachment) async -> Data?

    @State private var data: Data?
    @State private var image: UIImage?
    @State private var failed = false
    @State private var attempt = 0
    @State private var showFull = false

    var body: some View {
        if att.isImage {
            imageBody
                .task(id: attempt) { await load() }
                .fullScreenCover(isPresented: $showFull) {
                    if let data { ChatImageViewer(data: data, name: att.name) }
                }
        } else {
            fileRow
        }
    }

    @ViewBuilder private var imageBody: some View {
        if let image {
            Image(uiImage: image).resizable().scaledToFit()
                .frame(maxWidth: 240)
                .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
                .onTapGesture { showFull = true }
        } else if failed {
            VStack(spacing: 6) {
                Image(systemName: "arrow.clockwise").font(.system(size: 28)).foregroundStyle(Hey.goldInk(scheme))
                Text("Tap to load photo").font(.system(size: 12)).foregroundStyle(mine ? Hey.navy : Hey.ink(scheme))
            }
            .frame(width: 200, height: 130)
            .background(Hey.glassFill(scheme), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
            .onTapGesture { attempt += 1 }
        } else {
            ProgressView().tint(Hey.goldInk(scheme))
                .frame(width: 200, height: 130)
                .background(Hey.glassFill(scheme), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        }
    }

    private var fileRow: some View {
        HStack(spacing: 8) {
            Image(systemName: att.isVideo ? "play.fill" : "doc.text.fill")
                .foregroundStyle(mine ? Hey.navy : Hey.goldInk(scheme))
            VStack(alignment: .leading, spacing: 1) {
                Text(att.name.isEmpty ? "file" : att.name)
                    .font(HeyFont.callout).foregroundStyle(mine ? Hey.navy : Hey.ink(scheme))
                    .lineLimit(1)
                Text(heyHumanSize(att.size))
                    .font(HeyFont.timestamp).foregroundStyle(mine ? Hey.navy.opacity(0.7) : Hey.muted(scheme))
            }
        }
        .padding(.horizontal, 10).padding(.vertical, 8)
        .background(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .fill(mine ? Color.black.opacity(0.13) : Hey.glassFill(scheme))
        )
    }

    private func load() async {
        failed = false
        guard let raw = await fetch(att), !raw.isEmpty, let ui = UIImage(data: raw) else {
            failed = true; return
        }
        data = raw; image = ui
    }
}

/// Full-screen pinch-to-zoom viewer for a decrypted chat photo — port of the chat
/// FullImageViewer (MainActivity.kt:5244). Works on raw bytes (chat attachments
/// aren't a public CID, so the CID-based viewer doesn't apply).
private struct ChatImageViewer: View {
    @Environment(\.dismiss) private var dismiss
    let data: Data
    let name: String
    @State private var scale: CGFloat = 1
    @State private var offset: CGSize = .zero

    var body: some View {
        ZStack(alignment: .topTrailing) {
            Color.black.ignoresSafeArea()
            if let ui = UIImage(data: data) {
                Image(uiImage: ui).resizable().scaledToFit()
                    .scaleEffect(scale).offset(offset)
                    .gesture(MagnificationGesture().onChanged { scale = max(1, $0) }
                        .onEnded { _ in if scale < 1.05 { withAnimation { scale = 1; offset = .zero } } })
                    .simultaneousGesture(DragGesture().onChanged { if scale > 1 { offset = $0.translation } })
            }
            HStack {
                Button { saveToPhotos() } label: {
                    Image(systemName: "square.and.arrow.down").font(.system(size: 15, weight: .bold)).foregroundStyle(.white)
                        .padding(12).background(.black.opacity(0.4), in: Circle())
                }
                Button { dismiss() } label: {
                    Image(systemName: "xmark").font(.system(size: 15, weight: .bold)).foregroundStyle(.white)
                        .padding(12).background(.black.opacity(0.4), in: Circle())
                }
            }
            .padding(.top, 12).padding(.trailing, 12)
        }
    }

    private func saveToPhotos() {
        guard let ui = UIImage(data: data) else { return }
        UIImageWriteToSavedPhotosAlbum(ui, nil, nil, nil)
    }
}

/// A rectangle with independent corner radii — for the asymmetric chat-bubble tail
/// (Compose RoundedCornerShape(18,18,4,18) etc.).
private struct RoundedCornerShape: InsettableShape {
    var tl: CGFloat = 0, tr: CGFloat = 0, bl: CGFloat = 0, br: CGFloat = 0
    var inset: CGFloat = 0

    func path(in rect: CGRect) -> Path {
        let r = rect.insetBy(dx: inset, dy: inset)
        let tlr = min(tl, min(r.width, r.height) / 2)
        let trr = min(tr, min(r.width, r.height) / 2)
        let blr = min(bl, min(r.width, r.height) / 2)
        let brr = min(br, min(r.width, r.height) / 2)
        var p = Path()
        p.move(to: CGPoint(x: r.minX + tlr, y: r.minY))
        p.addLine(to: CGPoint(x: r.maxX - trr, y: r.minY))
        p.addArc(center: CGPoint(x: r.maxX - trr, y: r.minY + trr), radius: trr, startAngle: .degrees(-90), endAngle: .degrees(0), clockwise: false)
        p.addLine(to: CGPoint(x: r.maxX, y: r.maxY - brr))
        p.addArc(center: CGPoint(x: r.maxX - brr, y: r.maxY - brr), radius: brr, startAngle: .degrees(0), endAngle: .degrees(90), clockwise: false)
        p.addLine(to: CGPoint(x: r.minX + blr, y: r.maxY))
        p.addArc(center: CGPoint(x: r.minX + blr, y: r.maxY - blr), radius: blr, startAngle: .degrees(90), endAngle: .degrees(180), clockwise: false)
        p.addLine(to: CGPoint(x: r.minX, y: r.minY + tlr))
        p.addArc(center: CGPoint(x: r.minX + tlr, y: r.minY + tlr), radius: tlr, startAngle: .degrees(180), endAngle: .degrees(270), clockwise: false)
        p.closeSubpath()
        return p
    }

    func inset(by amount: CGFloat) -> some InsettableShape {
        var s = self; s.inset += amount; return s
    }
}
