import SwiftUI

/// Send BEAM / BEAMX — asset toggle + recipient + amount → review → confirm → broadcast.
/// Private Mimblewimble transfer; money-critical, so it keeps the explicit review step,
/// the safety-cap nudge, and a broadcast/confirmed/failed result screen.
/// Port of MainActivity.kt:3978-4135 (BeamSendSheet).
///
/// iOS engine note: the locked contract exposes `beamSend(token:amount:asset:)` (token =
/// recipient address, asset 0 = BEAM / 7 = BEAMX) returning `BeamSendResult{txid,status}`.
/// Android's `beamValidToken`, `beamCapLifted`, `beamTxStatus`, `BeamApi.toGroth`, and
/// `recordTx` are NOT on the iOS contract — we validate the amount client-side, show the
/// safety-cap note as static copy, and derive the result state from `BeamSendResult.status`.
/// (Live tx-status polling + address validation are TODOs pending engine methods.)
struct BeamSendSheet: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var scheme

    /// Dismiss without sending.
    var onClose: () -> Void
    /// A send completed and the user tapped Done.
    var onSent: () -> Void

    // asset 0 = BEAM, 7 = BEAMX (BeamApi.ASSET_BEAM / ASSET_BEAMX)
    private static let assetBeam = 0
    private static let assetBeamx = 7
    // Safety cap on first sends — mirrors BeamApi.SEND_CAP_BEAM (10 BEAM).
    private static let sendCapBeam = "10"

    @State private var asset = BeamSendSheet.assetBeam
    @State private var to = ""
    @State private var amount = ""
    @State private var busy = false
    @State private var status = ""
    @State private var confirm = false
    @State private var result: BeamSendResult?

    private var sym: String { asset == Self.assetBeamx ? "BEAMX" : "BEAM" }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    if let result {
                        resultView(result)
                    } else {
                        composer
                    }
                }
                .padding(20)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .scrollContentBackground(.hidden)
            .background(Hey.sheetBg(scheme).ignoresSafeArea())
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Close") { if !busy { onClose() } }
                        .tint(Hey.muted(scheme)).disabled(busy)
                }
            }
        }
        .presentationDetents([.large])
        .presentationDragIndicator(.visible)
        .interactiveDismissDisabled(busy)
        .alert("Confirm \(sym) transfer", isPresented: $confirm) {
            Button("Cancel", role: .cancel) {}.disabled(busy)
            Button("Send") { Task { await doSend() } }.disabled(busy)
        } message: {
            Text("\(amount) \(sym) to \(shortAddr(to))\n\nReal \(sym) on BEAM mainnet. It cannot be reversed.")
        }
    }

    // ── Composer (MainActivity.kt:4058-4107) ──
    private var composer: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Send \(sym)")
                .font(.system(size: 18, weight: .bold))
                .foregroundStyle(Hey.ink(scheme))
            Text("Private · Mimblewimble")
                .font(.system(size: 12))
                .foregroundStyle(Hey.muted(scheme))

            Spacer().frame(height: 16)

            // Asset toggle — same address; the asset is chosen here.
            HStack(spacing: 0) {
                assetTab(Self.assetBeam, "BEAM")
                assetTab(Self.assetBeamx, "BEAMX")
            }
            .padding(4)
            .background(Hey.glassFill(scheme), in: RoundedRectangle(cornerRadius: 12, style: .continuous))

            Spacer().frame(height: 14)

            // Recipient
            field(title: "Recipient BEAM address", text: $to, mono: true, keyboard: .default)

            Spacer().frame(height: 12)

            // Amount
            field(title: "Amount (\(sym))", text: $amount, mono: false, keyboard: .decimalPad, trailing: sym)

            Spacer().frame(height: 14)

            // Safety note (MainActivity.kt:4091-4099)
            HStack(alignment: .top, spacing: 8) {
                Image(systemName: "info.circle")
                    .font(.system(size: 18))
                    .foregroundStyle(Hey.goldInk(scheme))
                Text("This sends real \(sym) and can't be undone. For safety the first sends are capped at \(Self.sendCapBeam) BEAM — do a tiny test, then lift the cap in BEAM settings. Amounts below a cent may be rejected by the network.")
                    .font(.system(size: 12))
                    .foregroundStyle(Hey.muted(scheme))
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(12)
            .background(Hey.gold.opacity(0.10), in: RoundedRectangle(cornerRadius: 12, style: .continuous))

            if !status.isEmpty {
                Spacer().frame(height: 10)
                Text(status).font(.system(size: 13)).foregroundStyle(Hey.like)
            }

            Spacer().frame(height: 18)

            // Review & send
            Button { review() } label: {
                Group {
                    if busy {
                        ProgressView().controlSize(.small).tint(Hey.navy)
                    } else {
                        HStack(spacing: 8) {
                            Image(systemName: "paperplane.fill").font(.system(size: 18))
                            Text("Review & send").fontWeight(.bold)
                        }
                    }
                }
                .frame(maxWidth: .infinity).frame(height: 50)
            }
            .background(Hey.gold, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
            .foregroundStyle(Hey.navy)
            .disabled(busy || !store.engine.beamAvailable)

            Spacer().frame(height: 16)
        }
    }

    private func assetTab(_ a: Int, _ label: String) -> some View {
        let selected = asset == a
        return Text(label)
            .font(.system(size: 13, weight: .semibold))
            .foregroundStyle(selected ? Hey.navy : Hey.ink(scheme))
            .frame(maxWidth: .infinity).padding(.vertical, 9)
            .background(
                selected ? Hey.gold : .clear,
                in: RoundedRectangle(cornerRadius: 9, style: .continuous)
            )
            .contentShape(Rectangle())
            .onTapGesture { asset = a; status = "" }
    }

    private func field(title: String, text: Binding<String>, mono: Bool,
                       keyboard: UIKeyboardType, trailing: String? = nil) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title).font(.system(size: 12)).foregroundStyle(Hey.muted(scheme))
            HStack {
                TextField("", text: text)
                    .font(mono ? HeyFont.mono(12) : .system(size: 15))
                    .foregroundStyle(Hey.ink(scheme))
                    .keyboardType(keyboard)
                    .autocorrectionDisabled(mono)
                    .textInputAutocapitalization(.never)
                    .onChange(of: text.wrappedValue) { _ in status = "" }
                if let trailing {
                    Text(trailing).font(.system(size: 13)).foregroundStyle(Hey.muted(scheme))
                }
            }
            .padding(12)
            .glass(12)
        }
    }

    // ── Result screen (MainActivity.kt:4018-4057) ──
    @ViewBuilder
    private func resultView(_ r: BeamSendResult) -> some View {
        let state = confState(r.status)
        VStack(spacing: 0) {
            Group {
                switch state {
                case "confirmed":
                    Image(systemName: "checkmark.circle.fill")
                        .font(.system(size: 56)).foregroundStyle(Hey.good(scheme))
                case "failed":
                    Image(systemName: "exclamationmark.circle.fill")
                        .font(.system(size: 56)).foregroundStyle(Hey.like)
                default:
                    ProgressView().controlSize(.large).tint(Hey.goldInk(scheme))
                }
            }
            Spacer().frame(height: 12)
            Text(state == "confirmed" ? "Confirmed" : state == "failed" ? "Failed" : "Broadcast")
                .font(.system(size: 20, weight: .bold))
                .foregroundStyle(Hey.ink(scheme))
            Spacer().frame(height: 6)
            Text(resultBody(state))
                .font(.system(size: 13))
                .foregroundStyle(Hey.muted(scheme))
                .multilineTextAlignment(.center)
            Spacer().frame(height: 12)

            // tx id (copyable)
            Button {
                UIPasteboard.general.string = r.txid
            } label: {
                HStack(spacing: 6) {
                    Text("tx \(shortAddr(r.txid))")
                        .font(HeyFont.mono(12)).foregroundStyle(Hey.goldInk(scheme))
                    Image(systemName: "doc.on.doc")
                        .font(.system(size: 13)).foregroundStyle(Hey.muted(scheme))
                }
                .padding(.horizontal, 8).padding(.vertical, 4)
            }

            Spacer().frame(height: 20)
            Button(action: onSent) {
                Text("Done").fontWeight(.bold)
                    .frame(maxWidth: .infinity).padding(.vertical, 12)
            }
            .background(Hey.gold, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
            .foregroundStyle(Hey.navy)
            Spacer().frame(height: 16)
        }
        .frame(maxWidth: .infinity)
    }

    private func resultBody(_ state: String) -> String {
        switch state {
        case "confirmed": return "Your \(sym) transfer is confirmed."
        case "failed":    return "The transaction failed — your funds were not sent. Check the recipient and try again."
        default:          return "Sent to the network — confirming (Mimblewimble txs take a little longer)…"
        }
    }

    private func confState(_ status: String) -> String {
        switch status {
        case "confirmed", "ok", "success": return "confirmed"
        case "failed", "error":            return "failed"
        default:                            return "pending"
        }
    }

    // ── Actions ──

    /// Validate amount + recipient, then open the confirm dialog (MainActivity.kt:4003-4014).
    private func review() {
        let amt = amount.trimmingCharacters(in: .whitespaces)
        let recipient = to.trimmingCharacters(in: .whitespaces)
        guard let value = Double(amt), value > 0 else {
            status = "Enter an amount in \(sym)"; return
        }
        guard !recipient.isEmpty else {
            status = "Enter a recipient address"; return
        }
        // TODO: server-side address validation — no `beamValidToken` on the iOS contract yet.
        status = ""
        confirm = true
    }

    /// Build & broadcast (MainActivity.kt:3994-4002). The confirm alert is the auth gate.
    private func doSend() async {
        busy = true
        status = ""
        let recipient = to.trimmingCharacters(in: .whitespaces)
        let amt = amount.trimmingCharacters(in: .whitespaces)
        do {
            let r = try await store.engine.beamSend(token: recipient, amount: amt, asset: asset)
            result = r
            // Record locally for the wallet history — parity with Android's recordTx.
            await store.engine.recordTx(
                TxRecord(chain: "beam", symbol: sym, to: recipient, amount: amt, hash: r.txid))
            // TODO: poll live tx status — no `beamTxStatus` on the iOS contract. The
            // result screen reflects `BeamSendResult.status` (pending → confirmed/failed).
        } catch {
            status = error.localizedDescription
        }
        busy = false
    }
}

/// Short middle-ellipsis address — port of MainActivity.kt shortAddr.
private func shortAddr(_ s: String) -> String {
    s.count <= 14 ? s : "\(s.prefix(8))…\(s.suffix(6))"
}
