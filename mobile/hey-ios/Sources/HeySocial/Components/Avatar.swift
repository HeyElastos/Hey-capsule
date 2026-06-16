import SwiftUI

/// Circular avatar: gold gradient + initials, with an optional online status dot
/// (dock status dot: #35C759 online / #E5484D offline, MainActivity.kt:723-725).
struct Avatar: View {
    var name: String
    var size: CGFloat = 44
    var online: Bool? = nil
    var cid: String? = nil          // profile photo, resolved via the content provider

    private var initials: String {
        let parts = name.split(separator: " ")
        let chars = parts.prefix(2).compactMap { $0.first }
        return String(chars).uppercased()
    }

    private var gradientInitials: some View {
        Circle().fill(Hey.avatarGradient)
            .overlay(Text(initials).font(.system(size: size * 0.38, weight: .bold)).foregroundStyle(Hey.navy))
    }

    var body: some View {
        ZStack(alignment: .bottomTrailing) {
            Group {
                if let cid, !cid.isEmpty {
                    ContentImage(cid: cid) { gradientInitials }.scaledToFill()
                } else {
                    gradientInitials
                }
            }
            .frame(width: size, height: size)
            .clipShape(Circle())
            if let online {
                Circle()
                    .fill(online ? Hey.statusOnline : Hey.statusOffline)
                    .frame(width: size * 0.26, height: size * 0.26)
                    .overlay(Circle().strokeBorder(.background, lineWidth: 2))
            }
        }
    }
}
