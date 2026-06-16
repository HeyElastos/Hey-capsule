import SwiftUI

/// Frosted-glass surface — port of `Modifier.glass` (MainActivity.kt:156, radius 18).
/// Thin material + glassFill tint + hairline glassBorder.
struct GlassCard<Content: View>: View {
    @Environment(\.colorScheme) private var scheme
    var radius: CGFloat = HeyRadius.glass
    @ViewBuilder var content: Content

    var body: some View {
        content
            .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: radius, style: .continuous))
            .background(Hey.glassFill(scheme), in: RoundedRectangle(cornerRadius: radius, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: radius, style: .continuous)
                    .strokeBorder(Hey.glassBorder(scheme), lineWidth: 1)
            )
    }
}

extension View {
    /// `someView.glass()` for inline use.
    func glass(_ radius: CGFloat = HeyRadius.glass) -> some View {
        GlassCard(radius: radius) { self }
    }
}
