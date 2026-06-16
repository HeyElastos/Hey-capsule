import SwiftUI

// One row in the chat list — port of ChatRow (MainActivity.kt:4596-4623).
// Group rows show a gold-gradient Group glyph; 1:1 rows show the contact Avatar.
// Trailing column: relative time (only when ts > 0) + a Like-colored unread badge.
//
// Used only by ChatListView, but kept as a shared (non-private) component so the
// list and any future caller share one definition. Tap/long-press are mirrored as
// closures (the list owns navigation + the delete/mute/block confirm flow).
struct ChatRow: View {
    @Environment(\.colorScheme) private var scheme
    let chat: Chat
    var onTap: () -> Void = {}
    var onLongPress: () -> Void = {}

    var body: some View {
        HStack(spacing: HeySpace.md) {
            if chat.isGroup {
                Circle().fill(Hey.avatarGradient)
                    .frame(width: 46, height: 46)
                    .overlay(Image(systemName: "person.3.fill").font(.system(size: 18)).foregroundStyle(Hey.navy))
            } else {
                Avatar(name: chat.name, size: 46, online: chat.online, cid: chat.avatar)
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(chat.name)
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(Hey.ink(scheme))
                    .lineLimit(1)
                Text(chat.preview.isEmpty ? "Tap to chat" : chat.preview)
                    .font(HeyFont.caption)
                    .foregroundStyle(Hey.muted(scheme))
                    .lineLimit(1)
            }
            Spacer(minLength: 8)
            VStack(alignment: .trailing, spacing: 4) {
                if chat.ts > 0 {
                    Text(RelativeTime.relative(chat.ts))
                        .font(HeyFont.timestamp)
                        .foregroundStyle(Hey.muted(scheme))
                }
                if chat.unread > 0 {
                    Text("\(chat.unread)")
                        .font(.system(size: 12, weight: .bold))
                        .foregroundStyle(.white)
                        .padding(.horizontal, 7).padding(.vertical, 2)
                        .background(Hey.like, in: RoundedRectangle(cornerRadius: HeyRadius.reaction, style: .continuous))
                }
            }
        }
        .padding(12)
        .glass(14)
        .contentShape(Rectangle())
        .onTapGesture { onTap() }
        .onLongPressGesture { onLongPress() }
    }
}

/// Shared time formatters — port of relativeTime (MainActivity.kt:5914) and
/// clockTime (MainActivity.kt:5921). Both return "" for a non-positive timestamp,
/// matching the Android helpers (the list/bubble only render time when ts > 0).
enum RelativeTime {
    /// Relative span ("2m", "3h ago"), used in lists/cards.
    static func relative(_ ms: Int64) -> String {
        guard ms > 0 else { return "" }
        let date = Date(timeIntervalSince1970: Double(ms) / 1000)
        let f = RelativeDateTimeFormatter(); f.unitsStyle = .short
        return f.localizedString(for: date, relativeTo: Date())
    }

    /// Alias kept for existing call-sites (notifications/feed) that used `.short`.
    static func short(_ ms: Int64) -> String { relative(ms) }

    /// Short clock time (HH:mm) shown inside chat bubbles, Telegram-style.
    static func clock(_ ms: Int64) -> String {
        guard ms > 0 else { return "" }
        let f = DateFormatter(); f.dateFormat = "HH:mm"
        return f.string(from: Date(timeIntervalSince1970: Double(ms) / 1000))
    }
}

/// Human file size — port of humanSize (MainActivity.kt:3088).
func heyHumanSize(_ bytes: Int64) -> String {
    if bytes >= 1_000_000 { return String(format: "%.1f MB", Double(bytes) / 1_000_000.0) }
    if bytes >= 1_000 { return String(format: "%.0f KB", Double(bytes) / 1_000.0) }
    return "\(bytes) B"
}
