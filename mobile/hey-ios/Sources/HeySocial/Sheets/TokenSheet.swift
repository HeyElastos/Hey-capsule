import SwiftUI

// Port of TokenSheet (MainActivity.kt:3398-3459): native + curated ERC-20s on one EVM
// chain, with hide (scam protection) and tap-to-send. Curated list, so random
// airdropped scam tokens never appear. Tapping a token bubbles up to the send sheet
// (wallet-send group) via onSend.
struct TokenSheet: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var scheme
    @Environment(\.dismiss) private var dismiss

    let chain: WalletChain
    /// Bubble a tapped token up to the wallet-send group's send sheet (nil token = native).
    let onSend: (TokenBal?) -> Void

    @State private var tokens: [TokenBal] = []
    @State private var loading = true
    @State private var showHidden = false
    /// Locally hidden token contracts (scam protection). The engine contract has no
    /// hide method, so the UI keeps the hidden set in UserDefaults per chain.
    @State private var hidden: Set<String> = []

    private var hiddenKey: String { "hey.wallet.hiddenTokens.\(chain.key)" }

    private var visible: [TokenBal] {
        showHidden ? tokens : tokens.filter { $0.native || !hidden.contains($0.contract) }
    }
    private var hiddenN: Int { tokens.filter { !$0.native && hidden.contains($0.contract) }.count }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                Text("\(chain.title) tokens")
                    .font(.system(size: 18, weight: .bold))
                    .foregroundStyle(Hey.ink(scheme))
                Text("Tap a token to send it. Only verified tokens are shown.")
                    .font(.system(size: 12))
                    .foregroundStyle(Hey.muted(scheme))
                Spacer().frame(height: 14)

                if loading && tokens.isEmpty {
                    HStack { Spacer(); ProgressView().tint(Hey.goldInk(scheme)); Spacer() }
                        .padding(20)
                } else {
                    ForEach(visible) { t in
                        let isHidden = !t.native && hidden.contains(t.contract)
                        VStack(spacing: 0) {
                            Button { onSend(t.native ? nil : t) } label: { tokenRow(t, isHidden: isHidden) }
                                .buttonStyle(.plain)
                            Divider().overlay(Hey.glassBorder(scheme))
                        }
                    }

                    if hiddenN > 0 {
                        Spacer().frame(height: 6)
                        Button { showHidden.toggle() } label: {
                            HStack(spacing: 6) {
                                Image(systemName: showHidden ? "eye.slash" : "eye")
                                    .font(.system(size: 16))
                                Text(showHidden ? "Hide hidden tokens" : "Show \(hiddenN) hidden")
                                    .font(.system(size: 13))
                            }
                            .foregroundStyle(Hey.muted(scheme))
                        }
                        .buttonStyle(.plain)
                    }
                    Spacer().frame(height: 8)
                    Text("Tokens you didn't ask for? Hide them — a scammer can airdrop a fake token, but it can't move your funds.")
                        .font(.system(size: 11))
                        .foregroundStyle(Hey.muted(scheme))
                }
                Spacer().frame(height: 16)
            }
            .padding(20)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .scrollContentBackground(.hidden)
        .background(Hey.sheetBg(scheme).ignoresSafeArea())
        .presentationDetents([.medium, .large])
        .task(id: showHidden) { await load() }
    }

    private func tokenRow(_ t: TokenBal, isHidden: Bool) -> some View {
        HStack(spacing: 12) {
            ZStack {
                Circle().fill(Hey.avatarGradient).frame(width: 36, height: 36)
                Text(String(t.symbol.prefix(1)))
                    .font(.system(size: 15, weight: .bold))
                    .foregroundStyle(Hey.navy)
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(t.symbol)
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(Hey.ink(scheme))
                Text(t.native ? "\(chain.title) · native" : t.name)
                    .font(.system(size: 12))
                    .foregroundStyle(Hey.muted(scheme))
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
            Spacer(minLength: 4)
            Text(t.balance)
                .font(.system(size: 15, weight: .medium))
                .foregroundStyle(Hey.ink(scheme))
            if !t.native {
                Button { toggleHidden(t.contract) } label: {
                    Image(systemName: isHidden ? "eye" : "eye.slash")
                        .font(.system(size: 20))
                        .foregroundStyle(Hey.muted(scheme))
                }
                .buttonStyle(.plain)
                .padding(.leading, 4)
            }
        }
        .padding(.vertical, 10).padding(.horizontal, 4)
        .contentShape(Rectangle())
    }

    private func load() async {
        loading = true
        hidden = Set(UserDefaults.standard.stringArray(forKey: hiddenKey) ?? [])
        tokens = await store.engine.balances(chain: chain.key, includeHidden: showHidden)
        loading = false
    }

    private func toggleHidden(_ contract: String) {
        if hidden.contains(contract) { hidden.remove(contract) } else { hidden.insert(contract) }
        UserDefaults.standard.set(Array(hidden), forKey: hiddenKey)
    }
}
