import SwiftUI

// Activity / notifications feed. Port of Android's NotificationsScreen
// (MainActivity.kt:5944-6023): title "Activity", empty-state pointing at the
// QR/Profile share, and a tappable list of activity rows.
//
// The iOS engine surfaces typed notifications via `drainNotifs()` ->
// [HeyNotification] (kind = like | comment | follow | tip | mention | group),
// so this screen renders them with per-kind icons + tints and routes taps:
//   • follow → onOpenProfile(actor did)
//   • a notification carrying a postId → onOpenPost(postId)
//   • otherwise → onOpenProfile(actor did) when a did is present.
//
// Like Android, follow notifications offer a one-tap "Follow back". Rows carry an
// unread dot; tapping a row marks the whole batch seen for this session.
struct NotificationsScreen: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var scheme

    var topPad: CGFloat = 12
    var onOpenProfile: (String) -> Void = { _ in }
    var onOpenPost: (String) -> Void = { _ in }

    @State private var notifs: [HeyNotification] = []
    @State private var followingSet: Set<String> = []
    @State private var seen: Set<String> = []          // rows tapped this session (clears the dot)
    @State private var loaded = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Activity")
                .font(HeyFont.header)
                .foregroundStyle(Hey.ink(scheme))
                .padding(.horizontal, 18)
                .padding(.vertical, 14)

            if loaded && notifs.isEmpty {
                emptyState
            } else {
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(notifs) { n in
                            NotifRow(
                                notif: n,
                                following: followingSet.contains(n.did),
                                unread: !seen.contains(n.id),
                                onTap: { open(n) },
                                onFollowBack: { Task { await followBack(n.did) } }
                            )
                        }
                    }
                    .padding(.horizontal, 12)
                    .padding(.bottom, 96)   // clear the floating dock
                }
                .scrollContentBackground(.hidden)
            }
        }
        .padding(.top, topPad)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(FrostBackground())
        .task { await load() }
    }

    private var emptyState: some View {
        VStack(spacing: 0) {
            Image(systemName: "bell.fill")
                .font(.system(size: 46))
                .foregroundStyle(Hey.muted(scheme))
            Spacer().frame(height: 12)
            Text("No activity yet")
                .font(HeyFont.bodyCopy)
                .foregroundStyle(Hey.ink(scheme))
            Text("Share your QR (Profile) so people can follow you.")
                .font(HeyFont.caption)
                .foregroundStyle(Hey.muted(scheme))
                .multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(.horizontal, 24)
    }

    private func load() async {
        // drainNotifs() returns the pending batch; we also resolve which actors the
        // viewer already follows so "Follow back" only shows when relevant.
        async let pending = store.engine.drainNotifs()
        async let follows = (try? await store.engine.following()) ?? []
        notifs = await pending.sorted { $0.ts > $1.ts }
        followingSet = Set(await follows.map { $0.did })
        loaded = true
    }

    private func open(_ n: HeyNotification) {
        seen.insert(n.id)
        if !n.postId.isEmpty {
            onOpenPost(n.postId)
        } else if !n.did.isEmpty {
            onOpenProfile(n.did)
        }
    }

    private func followBack(_ did: String) async {
        guard !did.isEmpty else { return }
        try? await store.engine.followBack(did: did)
        followingSet.insert(did)
    }
}

// MARK: - Row

private struct NotifRow: View {
    @Environment(\.colorScheme) private var scheme
    let notif: HeyNotification
    let following: Bool
    let unread: Bool
    var onTap: () -> Void
    var onFollowBack: () -> Void

    private var displayName: String {
        if !notif.name.isEmpty { return notif.name }
        if !notif.did.isEmpty { return Profile.short(notif.did) }
        return "Someone"
    }

    var body: some View {
        Button(action: onTap) {
            HStack(spacing: 10) {
                // Avatar with a kind badge in the corner.
                ZStack(alignment: .bottomTrailing) {
                    Avatar(name: displayName, size: 38)
                    Image(systemName: kindIcon)
                        .font(.system(size: 10, weight: .bold))
                        .foregroundStyle(.white)
                        .padding(4)
                        .background(kindTint, in: Circle())
                        .overlay(Circle().strokeBorder(Hey.bg1(scheme), lineWidth: 1.5))
                        .offset(x: 3, y: 3)
                }
                VStack(alignment: .leading, spacing: 2) {
                    Text(displayName)
                        .font(HeyFont.author)
                        .foregroundStyle(Hey.ink(scheme))
                        .lineLimit(1)
                    Text(subtitle)
                        .font(HeyFont.caption)
                        .foregroundStyle(Hey.muted(scheme))
                        .lineLimit(1)
                }
                Spacer(minLength: 8)

                if notif.kind == "follow" {
                    if following {
                        Text("Following")
                            .font(HeyFont.timestamp)
                            .foregroundStyle(Hey.muted(scheme))
                    } else {
                        Button(action: onFollowBack) {
                            Text("Follow back")
                                .font(HeyFont.label)
                                .foregroundStyle(Hey.navy)
                                .padding(.horizontal, 14)
                                .padding(.vertical, 6)
                                .background(Hey.gold, in: Capsule())
                        }
                        .buttonStyle(.plain)
                    }
                } else if notif.ts > 0 {
                    Text(RelativeTime.short(notif.ts))
                        .font(HeyFont.timestamp)
                        .foregroundStyle(Hey.muted(scheme))
                }

                if unread {
                    Circle().fill(Hey.like).frame(width: 8, height: 8)
                }
            }
            .padding(12)
            .frame(maxWidth: .infinity)
            .glass(14)
        }
        .buttonStyle(.plain)
        .padding(.vertical, 4)
    }

    /// Fallback copy per kind when the engine doesn't supply `text` (Android shows
    /// "started following you" for follows).
    private var subtitle: String {
        if !notif.text.isEmpty { return notif.text }
        switch notif.kind {
        case "like":    return "liked your post"
        case "comment": return "commented on your post"
        case "follow":  return "started following you"
        case "tip":     return "sent you a tip"
        case "mention": return "mentioned you"
        case "group":   return "added you to a group"
        default:        return "new activity"
        }
    }

    private var kindIcon: String {
        switch notif.kind {
        case "like":    return "heart.fill"
        case "comment": return "bubble.right.fill"
        case "follow":  return "person.fill.badge.plus"
        case "tip":     return "dollarsign.circle.fill"
        case "mention": return "at"
        case "group":   return "person.3.fill"
        default:        return "bell.fill"
        }
    }

    private var kindTint: Color {
        switch notif.kind {
        case "like":    return Hey.like
        case "tip":     return Hey.gold
        case "comment": return Hey.goldInk(.dark)
        case "follow":  return Hey.good(scheme)
        default:        return Hey.goldInk(.dark)
        }
    }
}
