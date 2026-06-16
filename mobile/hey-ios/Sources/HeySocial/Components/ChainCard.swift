import SwiftUI

// A chain shown as a card in the wallet stack (port of UiChain, MainActivity.kt:3471).
// `evm` = full send+balance today (ESC/Ethereum/…); the Elastos mainchain is
// receive-only-ish (send via the ELA send sheet). Shared within the wallet group.
struct WalletChain: Identifiable, Hashable {
    let key: String        // "esc" | "ethereum" | "ela" | "beam"
    let title: String
    let sub: String
    let evm: Bool
    let symbol: String
    var id: String { key }
}

// Port of ChainCard (MainActivity.kt:3473-3521): a gold-gradient card with the chain
// header, big native balance + symbol, and a tap-to-copy short address. Tapping the
// card body opens tokens (EVM) or BEAM assets; the address chip copies.
struct ChainCard: View {
    @Environment(\.colorScheme) private var scheme
    let chain: WalletChain
    let address: String?
    let balance: String?
    let loading: Bool
    let onTap: () -> Void
    let onCopy: (String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if chain.evm || chain.key == "beam" {
                Text(chain.evm ? "Tap to view tokens" : "Tap to view assets")
                    .font(.system(size: 10))
                    .foregroundStyle(Hey.muted(scheme))
                    .frame(maxWidth: .infinity, alignment: .trailing)
                Spacer().frame(height: 2)
            }
            HStack(spacing: 0) {
                Image(systemName: chain.evm ? "bolt.fill" : "link")
                    .font(.system(size: 18))
                    .foregroundStyle(Hey.goldInk(scheme))
                Spacer().frame(width: 6)
                Text(chain.title)
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(Hey.ink(scheme))
                Spacer(minLength: 6)
                Text(chain.sub)
                    .font(.system(size: 11))
                    .foregroundStyle(Hey.muted(scheme))
            }

            Spacer().frame(height: 16)
            Text("Balance")
                .font(.system(size: 12))
                .foregroundStyle(Hey.muted(scheme))
            Spacer().frame(height: 4)

            if loading && balance == nil {
                ProgressView()
                    .tint(Hey.goldInk(scheme))
                    .frame(width: 26, height: 26)
            } else {
                HStack(alignment: .lastTextBaseline, spacing: 8) {
                    Text(balance ?? "—")
                        .font(.system(size: 38, weight: .bold))
                        .foregroundStyle(Hey.ink(scheme))
                    Text(chain.symbol)
                        .font(.system(size: 17, weight: .semibold))
                        .foregroundStyle(Hey.goldInk(scheme))
                        .padding(.bottom, 5)
                }
            }

            Spacer().frame(height: 16)
            if let address {
                Button { onCopy(address) } label: {
                    HStack(spacing: 8) {
                        Text(WalletFmt.shortAddr(address))
                            .font(HeyFont.mono(13))
                            .foregroundStyle(Hey.ink(scheme))
                        Image(systemName: "doc.on.doc")
                            .font(.system(size: 13))
                            .foregroundStyle(Hey.muted(scheme))
                    }
                    .padding(.horizontal, 12).padding(.vertical, 9)
                    .background(Color.black.opacity(0.13),
                                in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                }
                .buttonStyle(.plain)
            } else {
                Text("Deriving…")
                    .font(.system(size: 12))
                    .foregroundStyle(Hey.muted(scheme))
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(22)
        .background(
            LinearGradient(colors: [Hey.gold.opacity(0.22), Hey.gold.opacity(0.06)],
                           startPoint: .top, endPoint: .bottom),
            in: RoundedRectangle(cornerRadius: HeyRadius.sheet, style: .continuous)
        )
        .overlay(
            RoundedRectangle(cornerRadius: HeyRadius.sheet, style: .continuous)
                .strokeBorder(Hey.glassBorder(scheme), lineWidth: 1)
        )
        .contentShape(RoundedRectangle(cornerRadius: HeyRadius.sheet, style: .continuous))
        .onTapGesture { onTap() }
    }
}
