import SwiftUI

/// A tappable public-address row: a globe icon + label + monospaced address that
/// copies to the clipboard on tap (port of PublicAddrRow, MainActivity.kt:2628-2645).
/// Used in Settings / identity surfaces to show tip addresses, DID, etc.
struct PublicAddrRow: View {
    @Environment(\.colorScheme) private var scheme
    let label: String
    let addr: String
    var accent: Color = Hey.gold
    @State private var copied = false

    var body: some View {
        Button {
            UIPasteboard.general.string = addr
            withAnimation { copied = true }
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.4) {
                withAnimation { copied = false }
            }
        } label: {
            HStack(spacing: 6) {
                Image(systemName: "globe")
                    .font(.system(size: 14))
                    .foregroundStyle(accent)
                Text("\(label)  ")
                    .font(HeyFont.timestamp)
                    .foregroundStyle(Hey.muted(scheme))
                Text(addr)
                    .font(HeyFont.mono(11))
                    .foregroundStyle(Hey.ink(scheme))
                    .lineLimit(1)
                    .truncationMode(.tail)
                Spacer(minLength: 6)
                Image(systemName: copied ? "checkmark" : "doc.on.doc")
                    .font(.system(size: 12))
                    .foregroundStyle(copied ? Hey.good(scheme) : Hey.muted(scheme))
            }
            .padding(.vertical, 3)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}
