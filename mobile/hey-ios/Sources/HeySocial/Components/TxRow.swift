import SwiftUI

// Port of TxRow (MainActivity.kt:3310-3325): one recorded send/tip in the wallet's
// local history. Icon = paid for tips, send for everything else; subtitle is the
// truncated recipient + relative time. Below each row, a hairline divider.
struct TxRow: View {
    @Environment(\.colorScheme) private var scheme
    let tx: TxRecord

    private var isTip: Bool { tx.kind == "tip" }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                Image(systemName: isTip ? "dollarsign.circle.fill" : "paperplane.fill")
                    .font(.system(size: 18))
                    .foregroundStyle(Hey.goldInk(scheme))
                    .frame(width: 20, height: 20)
                VStack(alignment: .leading, spacing: 2) {
                    Text("\(isTip ? "Tipped" : "Sent") \(tx.amount) \(tx.symbol)")
                        .font(.system(size: 14, weight: .medium))
                        .foregroundStyle(Hey.ink(scheme))
                    Text("to \(WalletFmt.shortAddr(tx.to)) · \(RelativeTime.short(tx.ts))")
                        .font(.system(size: 11))
                        .foregroundStyle(Hey.muted(scheme))
                        .lineLimit(1)
                        .truncationMode(.tail)
                }
                Spacer(minLength: 0)
            }
            .padding(.vertical, 8)
            Divider().overlay(Hey.glassBorder(scheme))
        }
    }
}

/// Shared wallet formatting helpers (port of shortAddr, MainActivity.kt:3096, and the
/// decimal trim used by the chain cards). File-scoped name, but reused across the
/// wallet group so kept non-private inside the wallet files.
enum WalletFmt {
    /// "0xabc…123456" — first 8 + last 6 when longer than 14 chars.
    static func shortAddr(_ a: String) -> String {
        a.count > 14 ? "\(a.prefix(8))…\(a.suffix(6))" : a
    }
}
