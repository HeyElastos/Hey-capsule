import SwiftUI

/// BEAM private (Mimblewimble) wallet sheet — BEAM/BEAMX balances, maturing hint,
/// receive + send entry, and a quick balance sync against the official BEAM node.
/// Port of MainActivity.kt:3711-3789 (BeamAssetSheet).
///
/// iOS engine note: Android polls `beamSyncProgress()` on a process-scoped loop; the
/// iOS contract exposes a single `beamScan() -> BeamScanResult` (ok/synced/height/error)
/// that performs the quick sync and reports the outcome. We model "Sync balance" as one
/// scan call and reflect its result in the status line — same UX, contract-faithful.
struct BeamAssetSheet: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var scheme

    /// Open the receive (address QR) flow. Wired by the orchestrator.
    var onReceive: () -> Void
    /// Open the send flow. Wired by the orchestrator.
    var onSend: () -> Void
    /// Dismiss this sheet.
    var onClose: () -> Void

    @State private var bal: BeamBalance?
    @State private var syncing = false
    @State private var status = ""

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    Text("BEAM")
                        .font(.system(size: 20, weight: .bold))
                        .foregroundStyle(Hey.ink(scheme))
                    Text("Private Mimblewimble wallet")
                        .font(.system(size: 12))
                        .foregroundStyle(Hey.muted(scheme))

                    Spacer().frame(height: 14)

                    if store.engine.beamAvailable {
                        syncCard
                        Spacer().frame(height: 14)
                        BeamAssetRow("BEAM", bal?.beam ?? "—", maturing: bal?.beamMaturing)
                        Spacer().frame(height: 8)
                        BeamAssetRow("BEAMX", bal?.beamx ?? "—", sub: "confidential asset #7")
                        Spacer().frame(height: 16)
                        actionButtons
                        Spacer().frame(height: 8)
                        syncButton
                    } else {
                        unavailableCard
                    }

                    Spacer().frame(height: 24)
                }
                .padding(20)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .scrollContentBackground(.hidden)
            .background(Hey.sheetBg(scheme).ignoresSafeArea())
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Close") { onClose() }.tint(Hey.muted(scheme))
                }
            }
        }
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
        .task {
            guard store.engine.beamAvailable else { return }
            bal = await store.engine.beamBalance()
        }
    }

    // ── Quick-sync status card (MainActivity.kt:3750-3768) ──
    private var syncCard: some View {
        HStack(spacing: 8) {
            Image(systemName: "globe")
                .font(.system(size: 18))
                .foregroundStyle(Hey.goldInk(scheme))
            VStack(alignment: .leading, spacing: 1) {
                Text("Quick sync · official BEAM node")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(Hey.ink(scheme))
                Text(statusLine)
                    .font(.system(size: 11))
                    .foregroundStyle(Hey.muted(scheme))
            }
            Spacer(minLength: 0)
            if syncing {
                ProgressView()
                    .controlSize(.small)
                    .tint(Hey.goldInk(scheme))
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .glass(12)
    }

    private var statusLine: String {
        if !status.isEmpty { return status }
        return syncing ? "Syncing…" : "Tap Sync to update your balance"
    }

    // ── Send / Receive (MainActivity.kt:3774-3781) ──
    private var actionButtons: some View {
        HStack(spacing: 10) {
            Button(action: onSend) {
                HStack(spacing: 6) {
                    Image(systemName: "paperplane.fill").font(.system(size: 18))
                    Text("Send").fontWeight(.bold)
                }
                .frame(maxWidth: .infinity).padding(.vertical, 10)
            }
            .background(Hey.gold, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
            .foregroundStyle(Hey.navy)

            Button(action: onReceive) {
                HStack(spacing: 6) {
                    Image(systemName: "qrcode").font(.system(size: 18))
                    Text("Receive")
                }
                .frame(maxWidth: .infinity).padding(.vertical, 10)
                .foregroundStyle(Hey.ink(scheme))
            }
            .overlay(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .strokeBorder(Hey.glassBorder(scheme), lineWidth: 1)
            )
        }
    }

    // ── Sync balance (MainActivity.kt:3783-3785) ──
    private var syncButton: some View {
        Button { Task { await sync() } } label: {
            HStack(spacing: 6) {
                Image(systemName: "arrow.clockwise").font(.system(size: 18))
                Text(syncing ? "Syncing…" : "Sync balance").foregroundStyle(Hey.ink(scheme))
            }
            .frame(maxWidth: .infinity).padding(.vertical, 10)
        }
        .disabled(syncing)
        .overlay(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .strokeBorder(Hey.glassBorder(scheme), lineWidth: 1)
        )
    }

    private var unavailableCard: some View {
        HStack(spacing: 10) {
            Image(systemName: "lock.slash")
                .font(.system(size: 18))
                .foregroundStyle(Hey.muted(scheme))
            Text("BEAM is not included in this build.")
                .font(.system(size: 13))
                .foregroundStyle(Hey.muted(scheme))
            Spacer(minLength: 0)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14)
        .glass(12)
    }

    private func sync() async {
        guard !syncing else { return }
        syncing = true
        status = "Syncing…"
        let r = await store.engine.beamScan()
        let h = r.height > 0 ? " · block \(r.height)" : ""
        if let err = r.error, !err.isEmpty {
            status = err.replacingOccurrences(of: "beam: ", with: "")
        } else if r.synced {
            status = "Synced ✓\(h)"
            bal = await store.engine.beamBalance()
        } else if r.ok {
            status = "Syncing…\(h)"
            bal = await store.engine.beamBalance()
        } else {
            status = r.height > 0 ? "Last sync\(h)" : ""
        }
        syncing = false
    }
}
