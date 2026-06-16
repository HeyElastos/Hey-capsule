import SwiftUI

// Native ELA send on the Elastos MAINCHAIN (UTXO) — 1:1 port of MainActivity.kt
// ElaSendSheet (4384-4493). Recipient is an 'E…' address; the full byte-exact
// validation + P-256 signing happen in Rust, so the client does a light shape check
// and lets the engine re-validate. Self-contained (no chain/token args).
struct ElaSendSheet: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var scheme
    @Environment(\.dismiss) private var dismiss

    var onClose: () -> Void = {}
    var onSent: () -> Void = {}

    @State private var to = ""
    @State private var amount = ""
    @State private var busy = false
    @State private var status = ""
    @State private var confirm = false
    @State private var txHash: String? = nil

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                if let hash = txHash {
                    resultView(hash)
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
            Text("\(amount) ELA\nto \(shortAddr(to)) · Elastos Mainchain\n\nSigns with your key and broadcasts on the mainchain. It cannot be reversed.")
        }
    }

    private var editForm: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Send ELA").font(.system(size: 18, weight: .bold)).foregroundStyle(Hey.ink(scheme))
            Text("On the Elastos Mainchain").font(HeyFont.timestamp).foregroundStyle(Hey.muted(scheme))
            Spacer().frame(height: 18)

            ElaField(label: "Recipient address (E…)", text: $to, mono: true, onChange: { status = "" })
            Spacer().frame(height: 12)
            ElaField(label: "Amount (ELA)", text: $amount, decimal: true, trailing: "ELA", onChange: { status = "" })
            Spacer().frame(height: 14)

            ElaInfoBanner("This sends real ELA on the mainchain and can't be undone. Double-check the address — send a tiny amount first.")

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

    private func resultView(_ hash: String) -> some View {
        VStack(spacing: 0) {
            Image(systemName: "checkmark.circle.fill").font(.system(size: 56)).foregroundStyle(Hey.good(scheme))
            Spacer().frame(height: 12)
            Text("Broadcast").font(.system(size: 20, weight: .bold)).foregroundStyle(Hey.ink(scheme))
            Spacer().frame(height: 6)
            Text("Your ELA transfer is on the mainchain — it confirms in a couple of minutes.")
                .font(HeyFont.caption).foregroundStyle(Hey.muted(scheme)).multilineTextAlignment(.center)
            Spacer().frame(height: 12)
            Text("tx \(shortAddr(hash))").font(HeyFont.mono(12)).foregroundStyle(Hey.goldInk(scheme))
            Spacer().frame(height: 20)
            Button(action: onSent) {
                Text("Done").fontWeight(.bold).frame(maxWidth: .infinity, minHeight: 44)
                    .foregroundStyle(Hey.navy)
                    .background(Hey.gold, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
            }
            Spacer().frame(height: 16)
        }
        .frame(maxWidth: .infinity)
    }

    // edit → review: client-side 'E…' shape check; the Rust send re-validates byte-exact.
    private func review() {
        if !isElaAddress(to) {
            status = "Enter a valid Elastos mainchain address (starts with E)"; return
        }
        guard let amt = Double(amount.trimmingCharacters(in: .whitespaces)), amt > 0 else {
            status = "Enter an amount in ELA"; return
        }
        status = ""; confirm = true
    }

    // confirm → send: ELA spend grant bound to (ela,to,amount), then broadcast.
    private func doSend() {
        busy = true; status = "Authorizing…"
        Task {
            let grant = await store.engine.authorizeElaSend(to: to, amount: amount)
            if grant.isEmpty { busy = false; status = "Authorization cancelled"; return }
            status = "Signing & broadcasting…"
            do {
                let hash = try await store.engine.elaSend(to: to, amount: amount, auth: grant)
                busy = false; status = ""; txHash = hash
                await store.engine.recordTx(TxRecord(chain: "ela", symbol: "ELA", to: to, amount: amount, hash: hash))
            } catch {
                busy = false; status = error.localizedDescription
            }
        }
    }
}

// MARK: - Light client-side ELA address shape check
// TODO(unresolved): the contract has no isElaAddress(_:) — Android uses HeyApi.isElaAddress.
// This is a shape-only guard; the engine's elaSend re-validates byte-exact (P-256).
private func isElaAddress(_ s: String) -> Bool {
    let a = s.trimmingCharacters(in: .whitespaces)
    return a.hasPrefix("E") && a.count >= 25 && a.count <= 42
}

private func shortAddr(_ s: String) -> String {
    guard s.count > 12 else { return s }
    return "\(s.prefix(8))…\(s.suffix(4))"
}

// MARK: - Field + banner (file-private)

private struct ElaField: View {
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

private struct ElaInfoBanner: View {
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
