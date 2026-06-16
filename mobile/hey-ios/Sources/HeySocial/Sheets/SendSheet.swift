import SwiftUI

// EVM / ESC native + ERC-20 send sheet — 1:1 port of MainActivity.kt SendSheet (3808-3977).
//
// Three-step money flow: edit → review() (validate + checksum address) → confirm
// dialog → doSend() (authorize…Send → SpendGrant → …Send(auth:)). A returned hash
// only means the node accepted it, so we poll txStatus for real on-chain confirmation.
//
// `token == nil` → native send (authorizeEvmSend / walletSend);
// `token != nil` → ERC-20 send (authorizeTokenSend / tokenSend).
struct SendSheet: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var scheme
    @Environment(\.dismiss) private var dismiss

    let chain: String
    let symbol: String
    let network: String
    var token: TokenBal? = nil
    var onClose: () -> Void = {}
    var onSent: () -> Void = {}

    @State private var to = ""
    @State private var amount = ""
    @State private var busy = false
    @State private var status = ""
    @State private var confirm = false
    @State private var txHash: String? = nil

    private var sym: String { token?.symbol ?? symbol }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                if let hash = txHash {
                    SendResultView(chain: chain, txHash: hash, onDone: { onSent() })
                } else {
                    editForm
                }
            }
            .padding(20)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .scrollContentBackground(.hidden)
        .background(Hey.sheetBg(scheme).ignoresSafeArea())
        .presentationDetents([.medium, .large])
        .interactiveDismissDisabled(busy)
        .alert("Confirm transfer", isPresented: $confirm) {
            Button("Cancel", role: .cancel) { if !busy { confirm = false } }
            Button("Sign & send") { confirm = false; doSend() }.disabled(busy)
        } message: {
            Text("\(amount) \(sym)\nto \(shortAddr(to))\n\nThis signs with your key and broadcasts on \(network). It cannot be reversed.")
        }
    }

    private var editForm: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Send \(sym)").font(.system(size: 18, weight: .bold)).foregroundStyle(Hey.ink(scheme))
            Text("On \(network)").font(HeyFont.timestamp).foregroundStyle(Hey.muted(scheme))
            Spacer().frame(height: 18)

            GlassField(label: "Recipient address (0x…)", text: $to, mono: true,
                       onChange: { status = "" })
            Spacer().frame(height: 12)
            GlassField(label: "Amount (\(sym))", text: $amount, decimal: true,
                       trailing: sym, onChange: { status = "" })
            Spacer().frame(height: 14)

            InfoBanner("This sends real \(sym) and can't be undone. Double-check the address — and send a tiny amount first to be sure.")

            if !status.isEmpty {
                Spacer().frame(height: 10)
                Text(status).font(HeyFont.caption).foregroundStyle(Hey.like)
            }

            Spacer().frame(height: 18)
            Button(action: review) {
                HStack(spacing: 8) {
                    if busy {
                        ProgressView().tint(Hey.navy)
                    } else {
                        Image(systemName: "paperplane.fill").font(.system(size: 18))
                        Text("Review & send").fontWeight(.bold)
                    }
                }
                .frame(maxWidth: .infinity, minHeight: 50)
                .foregroundStyle(Hey.navy)
                .background(Hey.gold, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            }
            .disabled(busy)
            Spacer().frame(height: 16)
        }
    }

    // edit → review: validate amount + checksum/normalize the recipient before confirm.
    private func review() {
        guard let amt = Double(amount.trimmingCharacters(in: .whitespaces)), amt > 0 else {
            status = "Enter an amount in \(sym)"; return
        }
        busy = true; status = "Checking address…"
        Task {
            do {
                let checked = try await store.engine.checkAddress(to)
                to = checked; status = ""; busy = false; confirm = true
            } catch {
                busy = false; status = error.localizedDescription
            }
        }
    }

    // confirm → send: mint the spend grant bound to (kind,to,amount), then broadcast.
    private func doSend() {
        busy = true; status = "Authorizing…"
        Task {
            let grant: SpendGrant
            if let t = token {
                grant = await store.engine.authorizeTokenSend(chain: chain, contract: t.contract, to: to, amount: amount, decimals: t.decimals)
            } else {
                grant = await store.engine.authorizeEvmSend(chain: chain, to: to, amount: amount)
            }
            if grant.isEmpty { busy = false; status = "Authorization cancelled"; return }
            status = "Signing & broadcasting…"
            do {
                let hash: String
                if let t = token {
                    hash = try await store.engine.tokenSend(chain: chain, contract: t.contract, to: to, amount: amount, decimals: t.decimals, auth: grant)
                } else {
                    hash = try await store.engine.walletSend(chain: chain, to: to, amount: amount, auth: grant)
                }
                busy = false; status = ""; txHash = hash
                await store.engine.recordTx(TxRecord(chain: chain, symbol: sym, to: to, amount: amount, hash: hash))
            } catch {
                busy = false; status = error.localizedDescription
            }
        }
    }
}

// MARK: - Result view (shared shape; polls txStatus for real confirmation)

/// Post-broadcast confirmation. Polls txStatus (24× / 3s) so we report real on-chain
/// confirmation, not just that the node accepted the broadcast (Android audit #6).
private struct SendResultView: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var scheme
    let chain: String
    let txHash: String
    let onDone: () -> Void
    @State private var confState = "pending"

    var body: some View {
        VStack(spacing: 0) {
            switch confState {
            case "success": Image(systemName: "checkmark.circle.fill").font(.system(size: 56)).foregroundStyle(Hey.good(scheme))
            case "failed":  Image(systemName: "exclamationmark.octagon.fill").font(.system(size: 56)).foregroundStyle(Hey.like)
            default:        ProgressView().controlSize(.large).tint(Hey.goldInk(scheme))
            }
            Spacer().frame(height: 12)
            Text(confState == "success" ? "Confirmed" : confState == "failed" ? "Failed on-chain" : "Broadcast")
                .font(.system(size: 20, weight: .bold)).foregroundStyle(Hey.ink(scheme))
            Spacer().frame(height: 6)
            Text(resultMessage)
                .font(HeyFont.caption).foregroundStyle(Hey.muted(scheme)).multilineTextAlignment(.center)
            Spacer().frame(height: 12)
            Button {
                UIPasteboard.general.string = txHash
            } label: {
                HStack(spacing: 6) {
                    Text("tx \(shortAddr(txHash))").font(HeyFont.mono(12)).foregroundStyle(Hey.goldInk(scheme))
                    Image(systemName: "doc.on.doc").font(.system(size: 13)).foregroundStyle(Hey.muted(scheme))
                }
                .padding(.horizontal, 8).padding(.vertical, 4)
            }
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
            for _ in 0..<24 {
                try? await Task.sleep(nanoseconds: 3_000_000_000)
                let s = await store.engine.txStatus(chain: chain, hash: txHash)
                if s == "success" || s == "failed" { confState = s; return }
            }
        }
    }

    private var resultMessage: String {
        switch confState {
        case "success": return "Your transfer is confirmed on-chain."
        case "failed":  return "The transaction reverted on-chain — gas was spent but the funds were NOT sent. Re-check the recipient and try again."
        default:        return "Sent to the network — confirming on-chain (usually a few seconds)…"
        }
    }
}

// MARK: - Shared field + banner helpers (file-private to avoid cross-file collisions)

/// Outlined glass text field — port of glassFieldColors() + OutlinedTextField.
private struct GlassField: View {
    @Environment(\.colorScheme) private var scheme
    let label: String
    @Binding var text: String
    var mono = false
    var decimal = false
    var trailing: String? = nil
    var onChange: () -> Void = {}

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label).font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))
            HStack {
                TextField("", text: $text)
                    .font(mono ? HeyFont.mono(13) : HeyFont.body)
                    .foregroundStyle(Hey.ink(scheme))
                    .keyboardType(decimal ? .decimalPad : .default)
                    .autocorrectionDisabled(mono)
                    .textInputAutocapitalization(mono ? .never : .sentences)
                    .onChange(of: text) { _ in onChange() }
                if let t = trailing {
                    Text(t).font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))
                }
            }
            .padding(.horizontal, 14).padding(.vertical, 12)
            .background(Hey.glassFill(scheme), in: RoundedRectangle(cornerRadius: HeyRadius.attachment, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: HeyRadius.attachment, style: .continuous)
                .strokeBorder(Hey.glassBorder(scheme), lineWidth: 1))
        }
    }
}

/// The gold-tinted "this is real money" warning row.
private struct InfoBanner: View {
    @Environment(\.colorScheme) private var scheme
    let text: String
    init(_ text: String) { self.text = text }
    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            Image(systemName: "info.circle.fill").font(.system(size: 18)).foregroundStyle(Hey.goldInk(scheme))
            Text(text).font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Hey.gold.opacity(0.10), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
    }
}

/// Short hex/tx form: 0x1234…cdef (mirror of shortAddr in MainActivity.kt).
private func shortAddr(_ s: String) -> String {
    guard s.count > 12 else { return s }
    return "\(s.prefix(8))…\(s.suffix(4))"
}
