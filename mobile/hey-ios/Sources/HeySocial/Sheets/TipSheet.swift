import SwiftUI

// Tip a user BY IDENTITY — 1:1 port of MainActivity.kt TipSheet (4141-4378).
//
// "Sent by identity": Hey resolves the recipient's published wallet address over the
// carrier (refreshContact exchanges tip addresses over the DM channel, so it works even
// without following them). You pick a chain (ELA main chain / ESC / BEAM), optionally an
// ERC-20 asset on ESC, enter an amount, review → confirm → authorize → send → notifyTip.
struct TipSheet: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var scheme
    @Environment(\.dismiss) private var dismiss

    let authorDid: String
    let authorName: String
    var onClose: () -> Void = {}

    @State private var loading = true
    @State private var addresses: [String: String] = [:]
    @State private var chains: [ChainInfo] = []
    @State private var sel: ChainInfo? = nil
    @State private var amount = ""
    @State private var busy = false
    @State private var status = ""
    @State private var confirm = false
    @State private var txHash: String? = nil
    @State private var myTokens: [TokenBal] = []
    @State private var selTok: TokenBal? = nil   // nil = native
    @State private var retry = 0

    private var tipSym: String { selTok?.symbol ?? sel?.symbol ?? "" }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                if let hash = txHash {
                    TipResultView(chain: sel?.key ?? "", txHash: hash, authorName: authorName, onDone: { onClose() })
                } else {
                    header
                    Spacer().frame(height: 16)
                    bodyContent
                }
            }
            .padding(20)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .scrollContentBackground(.hidden)
        .background(Hey.sheetBg(scheme).ignoresSafeArea())
        .presentationDetents([.medium, .large])
        .interactiveDismissDisabled(busy)
        .task(id: retry) { await load() }
        .onChange(of: sel?.key) { _ in Task { await loadTokens() } }
        .alert("Confirm tip", isPresented: $confirm) {
            Button("Cancel", role: .cancel) { if !busy { confirm = false } }
            Button("Sign & tip") { confirm = false; doSend() }.disabled(busy)
        } message: {
            Text("\(amount) \(tipSym)\nto \(authorName) · on \(sel?.name ?? "")\n\nSigns with your key and broadcasts on-chain. It cannot be reversed.")
        }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                Image(systemName: "dollarsign.circle.fill").font(.system(size: 22)).foregroundStyle(Hey.goldInk(scheme))
                Text("Tip \(authorName)").font(.system(size: 18, weight: .bold)).foregroundStyle(Hey.ink(scheme))
            }
            Text("Sent by identity — Hey finds their address. You never need it.")
                .font(HeyFont.timestamp).foregroundStyle(Hey.muted(scheme))
        }
    }

    @ViewBuilder private var bodyContent: some View {
        if loading {
            HStack { Spacer(); ProgressView().tint(Hey.goldInk(scheme)); Spacer() }.padding(20)
        } else if chains.isEmpty {
            noAddressView
        } else {
            picker
        }
    }

    // No published address yet — there's no server; their address arrives with their profile.
    private var noAddressView: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("We don't have \(authorName)'s wallet address yet.")
                .font(.system(size: 15, weight: .semibold)).foregroundStyle(Hey.ink(scheme))
            Spacer().frame(height: 6)
            Text("There's no server — their address arrives with their profile over the network. If you follow them it usually syncs within moments (they may also need to update Hey). Try again in a bit.")
                .font(HeyFont.caption).foregroundStyle(Hey.muted(scheme)).lineSpacing(HeyLineSpacing.caption)
            Spacer().frame(height: 16)
            HStack(spacing: 10) {
                Button { retry += 1 } label: {
                    HStack(spacing: 6) {
                        Image(systemName: "arrow.clockwise").font(.system(size: 18))
                        Text("Try again").fontWeight(.bold)
                    }
                    .frame(maxWidth: .infinity, minHeight: 44)
                    .foregroundStyle(Hey.navy)
                    .background(Hey.gold, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                }
                Button(action: onClose) {
                    Text("Close").foregroundStyle(Hey.ink(scheme))
                        .frame(maxWidth: .infinity, minHeight: 44)
                        .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous)
                            .strokeBorder(Hey.glassBorder(scheme), lineWidth: 1))
                }
            }
            Spacer().frame(height: 8)
        }
    }

    private var picker: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Chain").font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))
            Spacer().frame(height: 6)
            // chips wrap to the next line (FlowRow parity).
            TipFlow(spacing: 8) {
                ForEach(chains) { c in
                    chip(label: chainLabel(c), on: sel?.key == c.key) { sel = c }
                }
            }

            // ERC-20 asset picker — only on ESC, and only if you hold >1 asset there.
            if sel?.key == "esc" && myTokens.count > 1 {
                Spacer().frame(height: 10)
                Text("Asset").font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))
                Spacer().frame(height: 6)
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 8) {
                        ForEach(myTokens) { t in
                            let on = t.native ? selTok == nil : selTok?.contract == t.contract
                            chip(label: t.symbol, on: on) { selTok = t.native ? nil : t }
                        }
                    }
                }
            }

            Spacer().frame(height: 12)
            TipField(label: "Amount (\(tipSym))", text: $amount, onChange: { status = "" })

            if !status.isEmpty {
                Spacer().frame(height: 10)
                Text(status).font(HeyFont.caption).foregroundStyle(Hey.like)
            }

            Spacer().frame(height: 16)
            Button(action: review) {
                HStack(spacing: 8) {
                    if busy {
                        ProgressView().tint(Hey.navy)
                    } else {
                        Image(systemName: "dollarsign.circle.fill").font(.system(size: 18))
                        Text("Review & tip").fontWeight(.bold)
                    }
                }
                .frame(maxWidth: .infinity, minHeight: 50)
                .foregroundStyle(Hey.navy)
                .background(Hey.gold, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            }
            .disabled(busy)
            Spacer().frame(height: 14)
        }
    }

    private func chip(label: String, on: Bool, tap: @escaping () -> Void) -> some View {
        Button(action: tap) {
            Text(label)
                .font(.system(size: 13, weight: on ? .semibold : .regular))
                .foregroundStyle(on ? Hey.goldInk(scheme) : Hey.ink(scheme))
                .padding(.horizontal, 14).padding(.vertical, 8)
                .background((on ? Hey.gold.opacity(0.22) : Hey.glassFill(scheme)),
                            in: RoundedRectangle(cornerRadius: 20, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: 20, style: .continuous)
                    .strokeBorder(on ? Hey.goldInk(scheme) : Hey.glassBorder(scheme), lineWidth: 1))
        }
    }

    private func chainLabel(_ c: ChainInfo) -> String {
        switch c.key {
        case "ela":  return "ELA · main chain"
        case "esc":  return "ESC"
        case "beam": return "BEAM"
        default:     return c.symbol
        }
    }

    // refreshContact exchanges tip addresses over the DM channel; build the tippable set.
    private func load() async {
        loading = true
        let a = await store.engine.refreshContact(did: authorDid)
        let my = await store.engine.walletChains()
        addresses = a
        var tippable: [ChainInfo] = []
        if a["ela"] != nil { tippable.append(ChainInfo(key: "ela", name: "ELA main chain", chainId: 0, symbol: "ELA")) }
        if let esc = my.first(where: { $0.key == "esc" }), a["esc"] != nil { tippable.append(esc) }
        if a["beam"] != nil && store.engine.beamAvailable { tippable.append(ChainInfo(key: "beam", name: "BEAM private", chainId: 0, symbol: "BEAM")) }
        chains = tippable
        sel = tippable.first
        loading = false
        await loadTokens()
    }

    // ERC-20 picker only applies on the EVM chain (ESC).
    private func loadTokens() async {
        selTok = nil
        if sel?.key == "esc" {
            myTokens = await store.engine.balances(chain: "esc")
        } else {
            myTokens = []
        }
    }

    // edit → review: amount + the PUBLISHED address sanity per chain (Rust re-checks).
    private func review() {
        guard let c = sel else { status = "Pick a chain"; return }
        guard let amt = Double(amount.trimmingCharacters(in: .whitespaces)), amt > 0 else {
            status = "Enter an amount"; return
        }
        guard let to = addresses[c.key], !to.isEmpty else {
            status = "They haven't published a \(c.symbol) address"; return
        }
        switch c.key {
        case "ela":
            if isElaAddress(to) { status = ""; confirm = true } else { status = "Their published address looks invalid" }
        case "beam":
            if to.count >= 16 { status = ""; confirm = true } else { status = "Their published address looks invalid" }
        default:
            busy = true; status = "Checking address…"
            Task {
                let ok = (try? await store.engine.checkAddress(to)) != nil
                busy = false
                if ok { status = ""; confirm = true } else { status = "Their published address looks invalid" }
            }
        }
    }

    // confirm → send: per-chain spend grant + broadcast, then notifyTip over the carrier.
    private func doSend() {
        guard let c = sel, let to = addresses[c.key] else { return }
        busy = true; status = "Authorizing…"
        Task {
            let t = selTok
            // BEAM has no guard grant; everything else mints a spend grant bound to (kind,to,amount).
            var grant: SpendGrant = ""
            if c.key != "beam" {
                switch c.key {
                case "ela":
                    grant = await store.engine.authorizeElaSend(to: to, amount: amount)
                default:
                    if let t, !t.native {
                        grant = await store.engine.authorizeTokenSend(chain: c.key, contract: t.contract, to: to, amount: amount, decimals: t.decimals)
                    } else {
                        grant = await store.engine.authorizeEvmSend(chain: c.key, to: to, amount: amount)
                    }
                }
                if grant.isEmpty { busy = false; status = "Authorization cancelled"; return }
            }
            status = "Signing & broadcasting…"
            do {
                let hash: String
                switch c.key {
                case "ela":
                    hash = try await store.engine.elaSend(to: to, amount: amount, auth: grant)
                case "beam":
                    hash = try await store.engine.beamSend(token: to, amount: amount, asset: 0).txid
                default:
                    if let t, !t.native {
                        hash = try await store.engine.tokenSend(chain: c.key, contract: t.contract, to: to, amount: amount, decimals: t.decimals, auth: grant)
                    } else {
                        hash = try await store.engine.walletSend(chain: c.key, to: to, amount: amount, auth: grant)
                    }
                }
                busy = false; status = ""; txHash = hash
                await store.engine.recordTx(TxRecord(chain: c.key, symbol: tipSym, to: authorName, amount: amount, hash: hash, kind: "tip"))
                // Tell the recipient over the carrier so they get a tip notification.
                await store.engine.notifyTip(to: authorDid, symbol: tipSym, amount: amount, txHash: hash)
            } catch {
                busy = false; status = error.localizedDescription
            }
        }
    }
}

// MARK: - Result view (polls txStatus for ESC; instant for ELA/BEAM)

private struct TipResultView: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var scheme
    let chain: String
    let txHash: String
    let authorName: String
    let onDone: () -> Void
    @State private var confState: String

    init(chain: String, txHash: String, authorName: String, onDone: @escaping () -> Void) {
        self.chain = chain; self.txHash = txHash; self.authorName = authorName; self.onDone = onDone
        _confState = State(initialValue: chain == "esc" ? "pending" : "success")
    }

    var body: some View {
        VStack(spacing: 0) {
            switch confState {
            case "success": Image(systemName: "checkmark.circle.fill").font(.system(size: 56)).foregroundStyle(Hey.good(scheme))
            case "failed":  Image(systemName: "exclamationmark.octagon.fill").font(.system(size: 56)).foregroundStyle(Hey.like)
            default:        ProgressView().controlSize(.large).tint(Hey.goldInk(scheme))
            }
            Spacer().frame(height: 12)
            Text(confState == "success" ? "Tipped \(authorName) 🎉" : confState == "failed" ? "Tip failed on-chain" : "Sending tip…")
                .font(.system(size: 19, weight: .bold)).foregroundStyle(Hey.ink(scheme))
                .multilineTextAlignment(.center)
            Spacer().frame(height: 20)
            Button(action: onDone) {
                Text("Done").fontWeight(.bold).frame(maxWidth: .infinity, minHeight: 44)
                    .foregroundStyle(Hey.navy)
                    .background(Hey.gold, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
            }
            Spacer().frame(height: 16)
        }
        .frame(maxWidth: .infinity)
        .task {
            guard chain == "esc" else { return }
            for _ in 0..<24 {
                try? await Task.sleep(nanoseconds: 3_000_000_000)
                let s = await store.engine.txStatus(chain: "esc", hash: txHash)
                if s == "success" || s == "failed" { confState = s; return }
            }
        }
    }
}

// MARK: - Light client-side ELA shape check (engine re-validates byte-exact)
// TODO(unresolved): the contract has no isElaAddress(_:) — Android uses HeyApi.isElaAddress.
private func isElaAddress(_ s: String) -> Bool {
    let a = s.trimmingCharacters(in: .whitespaces)
    return a.hasPrefix("E") && a.count >= 25 && a.count <= 42
}

// MARK: - Field + flow layout (file-private)

private struct TipField: View {
    @Environment(\.colorScheme) private var scheme
    let label: String
    @Binding var text: String
    var onChange: () -> Void = {}
    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label).font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))
            TextField("", text: $text)
                .font(HeyFont.body).foregroundStyle(Hey.ink(scheme))
                .keyboardType(.decimalPad)
                .onChange(of: text) { _ in onChange() }
                .padding(.horizontal, 14).padding(.vertical, 12)
                .background(Hey.glassFill(scheme), in: RoundedRectangle(cornerRadius: HeyRadius.attachment, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: HeyRadius.attachment, style: .continuous)
                    .strokeBorder(Hey.glassBorder(scheme), lineWidth: 1))
        }
    }
}

/// Minimal wrapping HStack (FlowRow parity) — chips flow onto the next line instead
/// of squeezing. iOS 16-safe (no iOS 17 Layout API needed for this simple case).
private struct TipFlow: Layout {
    var spacing: CGFloat = 8

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let maxW = proposal.width ?? .infinity
        var x: CGFloat = 0, y: CGFloat = 0, rowH: CGFloat = 0
        for v in subviews {
            let s = v.sizeThatFits(.unspecified)
            if x + s.width > maxW && x > 0 { x = 0; y += rowH + spacing; rowH = 0 }
            x += s.width + spacing
            rowH = max(rowH, s.height)
        }
        return CGSize(width: maxW == .infinity ? x : maxW, height: y + rowH)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var x = bounds.minX, y = bounds.minY, rowH: CGFloat = 0
        for v in subviews {
            let s = v.sizeThatFits(.unspecified)
            if x + s.width > bounds.maxX && x > bounds.minX { x = bounds.minX; y += rowH + spacing; rowH = 0 }
            v.place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(s))
            x += s.width + spacing
            rowH = max(rowH, s.height)
        }
    }
}
