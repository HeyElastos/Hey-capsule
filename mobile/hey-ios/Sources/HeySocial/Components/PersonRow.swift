import SwiftUI

// SHARED component (owner = group "activity"). A tappable glass row for a person,
// identified by DID. Port of Android `PersonRow` (MainActivity.kt:5928-5940):
// gold-gradient avatar with the first DID char, a shortened DID label, and an
// optional trailing accessory.
//
//   PersonRow(did:, name:, onTap:)                      — name shown as the label
//   PersonRow(did:, name:, onTap:) { trailingView }     — with a trailing accessory
//
// `name` falls back to the shortened DID when empty (Android shows the raw short DID).
struct PersonRow<Trailing: View>: View {
    @Environment(\.colorScheme) private var scheme
    let did: String
    var name: String = ""
    var onTap: () -> Void
    @ViewBuilder var trailing: () -> Trailing

    private var label: String {
        name.isEmpty ? Profile.short(did) : name
    }

    var body: some View {
        Button(action: onTap) {
            HStack(spacing: 10) {
                // Gold-gradient initial badge (Android: 34dp circle, first DID char).
                Circle()
                    .fill(Hey.avatarGradient)
                    .frame(width: 34, height: 34)
                    .overlay(
                        Text(Self.initial(did))
                            .font(.system(size: 14, weight: .bold))
                            .foregroundStyle(Hey.navy)
                    )
                Text(label)
                    .font(HeyFont.caption)
                    .foregroundStyle(Hey.ink(scheme))
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .frame(maxWidth: .infinity, alignment: .leading)
                trailing()
            }
            .padding(12)
            .frame(maxWidth: .infinity)
            .glass(14)
        }
        .buttonStyle(.plain)
        .padding(.vertical, 4)
    }

    /// First char of the DID after stripping the `did:key:z` prefix, upper-cased
    /// (Android: `did.removePrefix("did:key:z").take(1).uppercase()`).
    static func initial(_ did: String) -> String {
        let stripped = did.replacingOccurrences(of: "did:key:z", with: "")
        return String(stripped.prefix(1)).uppercased()
    }
}

// Convenience overload for rows with no trailing accessory.
extension PersonRow where Trailing == EmptyView {
    init(did: String, name: String = "", onTap: @escaping () -> Void) {
        self.init(did: did, name: name, onTap: onTap, trailing: { EmptyView() })
    }
}
