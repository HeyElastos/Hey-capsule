import SwiftUI

// "New group" sheet — 1:1 port of NewGroupSheet (MainActivity.kt:5427-5494).
// Name the group + multi-select from your existing 1:1 contacts, then createGroup.
struct NewGroupSheet: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var scheme

    var onClose: () -> Void = {}
    var onCreated: () -> Void = {}

    @State private var name = ""
    @State private var contacts: [Chat] = []
    @State private var selected: Set<String> = []
    @State private var busy = false
    @State private var status = ""

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                Text("New group")
                    .font(.system(size: 18, weight: .bold))
                    .foregroundStyle(Hey.ink(scheme))
                    .padding(.bottom, 12)

                TextField("", text: $name, prompt:
                    Text("Group name").foregroundColor(Hey.muted(scheme)))
                    .font(.system(size: 15))
                    .foregroundStyle(Hey.ink(scheme))
                    .padding(12)
                    .glass(12)

                Text("Add members")
                    .font(.system(size: 13))
                    .foregroundStyle(Hey.muted(scheme))
                    .padding(.top, 14).padding(.bottom, 6)

                if contacts.isEmpty {
                    Text("Add some contacts first — then you can group them.")
                        .font(.system(size: 13))
                        .foregroundStyle(Hey.muted(scheme))
                } else {
                    ForEach(contacts) { c in
                        MemberRow(chat: c, selected: selected.contains(c.id)) {
                            if selected.contains(c.id) { selected.remove(c.id) }
                            else { selected.insert(c.id) }
                        }
                    }
                }

                if !status.isEmpty {
                    Text(status)
                        .font(.system(size: 13))
                        .foregroundStyle(Hey.like)
                        .padding(.top, 8)
                }

                Button(action: create) {
                    Group {
                        if busy {
                            ProgressView().tint(Hey.navy)
                        } else {
                            Text("Create group").font(.system(size: 15, weight: .bold)).foregroundStyle(Hey.navy)
                        }
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 13)
                    .background(Hey.gold, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                }
                .buttonStyle(.plain)
                .disabled(busy)
                .padding(.top, 16)
            }
            .padding(20)
            .padding(.bottom, 24)
        }
        .scrollContentBackground(.hidden)
        .background(Hey.sheetBg(scheme).ignoresSafeArea())
        .presentationDetents([.large])
        .presentationDragIndicator(.visible)
        .task {
            contacts = ((try? await store.engine.chats()) ?? []).filter { !$0.isGroup }
        }
    }

    private func create() {
        let n = name.trimmingCharacters(in: .whitespacesAndNewlines)
        if n.isEmpty { status = "Name the group"; return }
        if selected.isEmpty { status = "Pick at least one member"; return }
        busy = true; status = ""
        let members = Array(selected)
        Task {
            let id = try? await store.engine.createGroup(name: n, members: members)
            busy = false
            if id != nil { onCreated(); onClose() }
            else { status = "Couldn't create group" }
        }
    }
}

// A selectable contact row (MainActivity.kt:5453-5468).
private struct MemberRow: View {
    @Environment(\.colorScheme) private var scheme
    let chat: Chat
    let selected: Bool
    let onTap: () -> Void

    var body: some View {
        Button(action: onTap) {
            HStack(spacing: 10) {
                Avatar(name: chat.name, size: 38, cid: chat.avatar)
                Text(chat.name)
                    .font(.system(size: 15))
                    .foregroundStyle(Hey.ink(scheme))
                Spacer(minLength: 0)
                Image(systemName: selected ? "checkmark.circle.fill" : "circle")
                    .foregroundStyle(selected ? Hey.goldInk(scheme) : Hey.muted(scheme))
            }
            .padding(10)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .glass(12)
        .padding(.vertical, 4)
    }
}
