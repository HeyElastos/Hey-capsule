import SwiftUI

// The bottom floating dock — 5 tabs (Chat / Feed / Verse / Wallet / You), 28pt radius,
// gold@0.18 highlight pill (20pt), label only on the selected tab. When the Verse tab
// is active the dock MORPHS into the game's controls (Avatar / Worlds / Invite /
// Library / Exit). Activity is a top-right bell, NOT a dock tab.
// Verified against MainActivity.kt:1197-1248.

enum HeyTab: Int, CaseIterable, Identifiable {
    case chat, feed, verse, wallet, you
    var id: Int { rawValue }
    var title: String { ["Chat", "Feed", "Verse", "Wallet", "You"][rawValue] }
    // SF Symbols mapped from the Android Material icons (Forum / DynamicFeed / Public /
    // AccountBalanceWallet / AccountCircle).
    var icon: String {
        ["bubble.left.and.bubble.right.fill", "rectangle.stack.fill", "globe",
         "creditcard.fill", "person.crop.circle.fill"][rawValue]
    }
}

/// Verse dock-morph actions (mirrors onVerse("avatar"|"worlds"|"invite"|"library"|"exit")).
enum VerseDockAction: String, CaseIterable, Identifiable {
    case avatar, worlds, invite, library, exit
    var id: String { rawValue }
    var title: String { rawValue.capitalized }
    var icon: String {
        switch self {
        case .avatar:  return "face.smiling"
        case .worlds:  return "globe"
        case .invite:  return "person.badge.plus"
        case .library: return "archivebox.fill"
        case .exit:    return "power"
        }
    }
}

struct FloatingDock: View {
    @Environment(\.colorScheme) private var scheme
    @Binding var selected: HeyTab
    var unread: Int = 0
    var online: Bool = true
    var onVerse: (VerseDockAction) -> Void = { _ in }

    var body: some View {
        HStack(spacing: 4) {
            if selected == .verse {
                ForEach(VerseDockAction.allCases) { a in
                    dockButton(icon: a.icon, label: a.title, selected: false, badge: 0, status: nil) { onVerse(a) }
                }
                .transition(.move(edge: .bottom).combined(with: .opacity))
            } else {
                ForEach(HeyTab.allCases) { tab in
                    dockButton(icon: tab.icon, label: tab.title,
                               selected: tab == selected,
                               badge: tab == .chat ? unread : 0,
                               status: tab == .you ? online : nil) {
                        withAnimation(.easeInOut(duration: 0.28)) { selected = tab }
                    }
                }
                .transition(.move(edge: .top).combined(with: .opacity))
            }
        }
        .padding(.horizontal, 10).padding(.vertical, 8)
        .background(Hey.bg2(scheme).opacity(0.95), in: RoundedRectangle(cornerRadius: HeyRadius.dock, style: .continuous))
        .background(
            LinearGradient(colors: [.white.opacity(scheme == .light ? 0.22 : 0.09),
                                    .white.opacity(scheme == .light ? 0.06 : 0.02)],
                           startPoint: .top, endPoint: .bottom),
            in: RoundedRectangle(cornerRadius: HeyRadius.dock, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: HeyRadius.dock, style: .continuous).strokeBorder(Hey.glassBorder(scheme), lineWidth: 1))
        .shadow(color: .black.opacity(0.18), radius: 18, y: 8)
        .padding(.horizontal, 22)
        .animation(.easeInOut(duration: 0.26), value: selected == .verse)
    }

    @ViewBuilder
    private func dockButton(icon: String, label: String, selected: Bool, badge: Int, status: Bool?, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 7) {
                ZStack(alignment: .topTrailing) {
                    Image(systemName: icon).font(.system(size: 22, weight: .semibold))
                    if badge > 0 {
                        Text(badge > 99 ? "99+" : "\(badge)")
                            .font(.system(size: 9, weight: .bold)).foregroundStyle(.white)
                            .padding(.horizontal, 4).padding(.vertical, 1)
                            .background(Hey.like, in: Capsule())
                            .overlay(Capsule().strokeBorder(Hey.bg2(scheme), lineWidth: 1.5))
                            .offset(x: 9, y: -6)
                    } else if let status {
                        Circle().fill(status ? Hey.statusOnline : Hey.statusOffline)
                            .frame(width: 9, height: 9)
                            .overlay(Circle().strokeBorder(Hey.bg2(scheme), lineWidth: 1.5))
                            .offset(x: 4, y: -2)
                    }
                }
                if selected {
                    Text(label).font(HeyFont.label)
                        .transition(.opacity.combined(with: .move(edge: .leading)))
                }
            }
            .foregroundStyle(selected ? Hey.goldInk(scheme) : Hey.muted(scheme))
            .padding(.vertical, 10)
            .padding(.horizontal, selected ? 16 : 12)
            .background(
                RoundedRectangle(cornerRadius: HeyRadius.dockItem, style: .continuous)
                    .fill(selected ? Hey.gold.opacity(0.18) : .clear)
            )
            .animation(.easeInOut(duration: 0.28), value: selected)
        }
        .buttonStyle(.plain)
        .frame(maxWidth: .infinity)
    }
}
