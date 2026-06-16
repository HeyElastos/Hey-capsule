import SwiftUI

// Chat list — port of ChatListScreen (MainActivity.kt:4529-4593).
// • Polls hey_chats every 2s into rows (avatar/name/preview/time/unread).
// • Empty state: "No conversations yet" + hint.
// • Long-press a row → delete/leave confirm (engine.deleteChat).
// • Floating "+" pair: small = new group (onNewGroup), gold = add contact (onAddContact).
// • Tap a row → onOpen(chat). The header keeps the verse globe + activity bell.
//
// Navigation is NOT ours: onOpen / onAddContact / onNewGroup are wired by the
// orchestrator in RootView. openVerse is kept from the previous screen.
struct ChatListView: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var scheme

    var onOpen: (Chat) -> Void = { _ in }
    var onAddContact: () -> Void = {}
    var onNewGroup: () -> Void = {}
    var openVerse: () -> Void = {}

    @State private var chats: [Chat] = []
    @State private var loaded = false
    @State private var toDelete: Chat?
    @State private var showNotifications = false
    @State private var hasUnreadActivity = true

    var body: some View {
        NavigationStack {
            ZStack(alignment: .bottomTrailing) {
                if loaded && chats.isEmpty {
                    emptyState
                } else {
                    ScrollView {
                        LazyVStack(spacing: 0) {
                            ForEach(chats) { c in
                                ChatRow(chat: c,
                                        onTap: { onOpen(c) },
                                        onLongPress: { toDelete = c })
                                    .padding(.vertical, 5)
                            }
                        }
                        .padding(.horizontal, 12)
                        .padding(.bottom, 96)   // clear the floating dock
                    }
                    .scrollContentBackground(.hidden)
                    .refreshable { await load() }
                }
                fabs
            }
            .navigationTitle("Chat")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Text("Chat").font(HeyFont.header).foregroundStyle(Hey.ink(scheme))
                }
                ToolbarItemGroup(placement: .topBarTrailing) {
                    Button { openVerse() } label: { Image(systemName: "globe.americas.fill") }
                    Button { hasUnreadActivity = false; showNotifications = true } label: {
                        Image(systemName: "bell.fill")
                            .overlay(alignment: .topTrailing) {
                                if hasUnreadActivity {
                                    Circle().fill(Hey.like).frame(width: 7, height: 7).offset(x: 3, y: -2)
                                }
                            }
                    }
                }
            }
            .sheet(isPresented: $showNotifications) { NotificationsScreen() }
            .confirmationDialog(
                toDelete.map { $0.isGroup ? "Leave & delete group?" : "Delete conversation?" } ?? "",
                isPresented: Binding(get: { toDelete != nil }, set: { if !$0 { toDelete = nil } }),
                titleVisibility: .visible
            ) {
                Button("Delete", role: .destructive) {
                    if let c = toDelete { delete(c) }
                    toDelete = nil
                }
                Button("Cancel", role: .cancel) { toDelete = nil }
            } message: {
                if let c = toDelete { Text(c.name) }
            }
            .task { await poll() }
        }
        .tint(Hey.goldInk(scheme))
    }

    // ── empty state (MainActivity.kt:4549-4555) ──
    private var emptyState: some View {
        VStack(spacing: 0) {
            Image(systemName: "bubble.left.and.bubble.right.fill")
                .font(.system(size: 48)).foregroundStyle(Hey.muted(scheme))
            Spacer().frame(height: 12)
            Text("No conversations yet")
                .font(.system(size: 17, weight: .semibold)).foregroundStyle(Hey.ink(scheme))
            Text("Tap + to message someone you follow, or paste a friend link.")
                .font(HeyFont.body).foregroundStyle(Hey.muted(scheme))
                .multilineTextAlignment(.center)
                .padding(.horizontal, 32)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    // ── floating action buttons (MainActivity.kt:4563-4571) ──
    private var fabs: some View {
        VStack(spacing: 12) {
            Button { onNewGroup() } label: {
                Image(systemName: "person.2.badge.plus.fill")
                    .font(.system(size: 18)).foregroundStyle(Hey.goldInk(scheme))
                    .frame(width: 44, height: 44)
                    .background(Hey.sheetBg(scheme), in: Circle())
                    .shadow(color: .black.opacity(0.18), radius: 6, y: 3)
            }
            Button { onAddContact() } label: {
                Image(systemName: "person.crop.circle.badge.plus")
                    .font(.system(size: 22)).foregroundStyle(Hey.navy)
                    .frame(width: 56, height: 56)
                    .background(Hey.gold, in: Circle())
                    .shadow(color: .black.opacity(0.22), radius: 8, y: 4)
            }
        }
        .padding(.trailing, 20)
        .padding(.bottom, 96)
    }

    // Poll the chat list every 2s (MainActivity.kt:4538-4547).
    private func poll() async {
        while !Task.isCancelled {
            await load()
            try? await Task.sleep(nanoseconds: 2_000_000_000)
        }
    }

    private func load() async {
        let next = (try? await store.engine.chats()) ?? []
        if next != chats { chats = next }
        loaded = true
    }

    private func delete(_ c: Chat) {
        Task {
            await store.engine.deleteChat(c)
            await load()
        }
    }
}
