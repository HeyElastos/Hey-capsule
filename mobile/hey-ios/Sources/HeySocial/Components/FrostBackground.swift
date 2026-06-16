import SwiftUI

/// The app-wide frosted backdrop: base bg + three soft glow blobs
/// (port of FrostBackground, MainActivity.kt:160-176). Works in dark + light.
struct FrostBackground: View {
    @Environment(\.colorScheme) private var scheme
    var body: some View {
        let (g1, g2, g3) = Hey.glow(scheme)
        ZStack {
            Hey.bg1(scheme).ignoresSafeArea()
            GeometryReader { geo in
                let w = geo.size.width, h = geo.size.height
                blob(g1).frame(width: w * 0.9).position(x: w * 0.2, y: h * 0.12)
                blob(g2).frame(width: w * 1.1).position(x: w * 0.9, y: h * 0.35)
                blob(g3).frame(width: w * 0.8).position(x: w * 0.15, y: h * 0.85)
            }
            .ignoresSafeArea()
            .blur(radius: 60)
        }
    }
    private func blob(_ c: Color) -> some View {
        Circle().fill(RadialGradient(colors: [c, .clear], center: .center, startRadius: 0, endRadius: 220))
    }
}
