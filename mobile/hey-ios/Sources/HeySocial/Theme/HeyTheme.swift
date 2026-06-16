import SwiftUI

// Design tokens — verified 1:1 against the Android app's MainActivity.kt.
// Source of truth: docs/HEY_IOS_UI_PORT.md. Theme-constant accents are plain
// statics; theme-varying colors take a ColorScheme (read @Environment(\.colorScheme)).

extension Color {
    init(hex: UInt32, alpha: Double = 1.0) {
        self.init(.sRGB,
                  red:   Double((hex >> 16) & 0xFF) / 255,
                  green: Double((hex >> 8) & 0xFF) / 255,
                  blue:  Double(hex & 0xFF) / 255,
                  opacity: alpha)
    }
}

enum Hey {
    // ── Theme-CONSTANT accents (MainActivity.kt:95-99) ──
    static let gold  = Color(hex: 0xD4B84B)   // primary accent, FAB, button fills, sent bubble
    static let gold2 = Color(hex: 0xFACC15)   // bright gold, avatar-gradient end, glow
    static let like  = Color(hex: 0xFF5A7A)   // like / danger / unread badge
    static let navy  = Color(hex: 0x091427)   // dark text/icon that sits ON gold (both themes)

    // ── Theme-VARYING (light, dark) (MainActivity.kt:105-118) ──
    static func bg1(_ s: ColorScheme)         -> Color { s == .light ? Color(hex: 0xF6F7FB) : Color(hex: 0x0B1A36) }
    static func bg2(_ s: ColorScheme)         -> Color { s == .light ? Color(hex: 0xEDEFF5) : Color(hex: 0x071021) }
    static func bg3(_ s: ColorScheme)         -> Color { s == .light ? Color(hex: 0xDFE4EE) : Color(hex: 0x040A14) }
    static func ink(_ s: ColorScheme)         -> Color { s == .light ? Color(hex: 0x13213B) : Color(hex: 0xEAF0FA) }
    static func muted(_ s: ColorScheme)       -> Color { s == .light ? Color(hex: 0x5B6B86) : Color(hex: 0x8DA0BE) }
    static func glassFill(_ s: ColorScheme)   -> Color { s == .light ? Color(hex: 0x0B1A36, alpha: 0x0F / 255.0)
                                                                      : Color(hex: 0xFFFFFF, alpha: 0x0E / 255.0) }
    static func glassBorder(_ s: ColorScheme) -> Color { s == .light ? Color(hex: 0x0B1A36, alpha: 0x1F / 255.0)
                                                                      : Color(hex: 0xFFFFFF, alpha: 0x1A / 255.0) }
    static func sheetBg(_ s: ColorScheme)     -> Color { s == .light ? .white : Color(hex: 0x0C1A33) }
    static func goldInk(_ s: ColorScheme)     -> Color { s == .light ? Color(hex: 0x8A6D12) : gold } // gold-as-text on light
    static func good(_ s: ColorScheme)        -> Color { s == .light ? Color(hex: 0x1E9E54) : Color(hex: 0x78E68C) }
    static func bubbleIn(_ s: ColorScheme)    -> Color { s == .light ? .white : Color(hex: 0xFFFFFF, alpha: 0x14 / 255.0) }

    // ── Status / call (MainActivity.kt:516, 567-568, 595, 605, 724) ──
    static let statusOnline  = Color(hex: 0x35C759)
    static let statusOffline = Color(hex: 0xE5484D)
    static let callReject    = Color(hex: 0xE5484D)
    static let callAccept    = Color(hex: 0x1FAD66)
    static let callBtnIdle   = Color(hex: 0xFFFFFF, alpha: 0x33 / 255.0)
    static let callGradStart = Color(hex: 0x0A1426)
    static let callGradEnd   = Color(hex: 0x13233F)

    static func glow(_ s: ColorScheme) -> (Color, Color, Color) {
        s == .light
        ? (Color(hex: 0xFACC15, alpha: 0.12), Color(hex: 0x8FB8E0, alpha: 0.22), Color(hex: 0xB6A6E8, alpha: 0.12))
        : (Color(hex: 0xD4B84B, alpha: 0.16), Color(hex: 0x2A6FB0, alpha: 0.20), Color(hex: 0x7A4FD0, alpha: 0.12))
    }

    static let avatarGradient = LinearGradient(
        colors: [gold, gold2], startPoint: .topLeading, endPoint: .bottomTrailing)
}

// Type scale (MainActivity.kt:319, 417-418, 731, 835, 3781, 4166-4374).
enum HeyFont {
    static let display   = Font.system(size: 52, weight: .bold)      // "Hey" title
    static let header    = Font.system(size: 26, weight: .bold)
    static let subtitle  = Font.system(size: 22, weight: .light)
    static let bodyCopy  = Font.system(size: 16, weight: .medium)    // lineSpacing 6
    static let body      = Font.system(size: 15)                     // message text
    static let author    = Font.system(size: 14, weight: .semibold)
    static let callout   = Font.system(size: 14)                     // lineSpacing 7
    static let label     = Font.system(size: 13, weight: .semibold)  // tab / chip / button
    static let caption   = Font.system(size: 13)                     // lineSpacing 5
    static let timestamp = Font.system(size: 11)
    static let tick      = Font.system(size: 10)
    static let mono      = Font.system(.body, design: .monospaced)   // DIDs / phrases / addresses
    static func mono(_ size: CGFloat) -> Font { .system(size: size, design: .monospaced) }
}

// SwiftUI has no per-Text lineHeight; emulate Compose lineHeight with .lineSpacing (lineHeight - fontSize).
enum HeyLineSpacing { static let bodyCopy: CGFloat = 6; static let callout: CGFloat = 7; static let caption: CGFloat = 5 }

// Shapes (MainActivity.kt:156, 479, 673, 704, 742, 3804, 3807, 3853).
enum HeyRadius {
    static let glass: CGFloat      = 18  // default glass card
    static let sheet: CGFloat      = 22  // modal / composer
    static let dock: CGFloat       = 28  // floating dock  [verified, not 24]
    static let dockItem: CGFloat   = 20  // tab pill
    static let attachment: CGFloat = 12
    static let reaction: CGFloat   = 11
    static let thumb: CGFloat      = 7
}

enum HeySpace {
    static let xs: CGFloat = 4
    static let sm: CGFloat = 8
    static let md: CGFloat = 12
    static let lg: CGFloat = 16
    static let xl: CGFloat = 24
}
