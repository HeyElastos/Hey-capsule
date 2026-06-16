import SwiftUI

// Port of WalletSettingsSheet (MainActivity.kt:3326-3373): show/hide the local
// transaction history, plus how the BEAM (private) wallet syncs. The toggles persist
// in UserDefaults (Android uses SharedPreferences via HeyApi.showTxHistory etc.).
struct WalletSettingsSheet: View {
    @Environment(\.colorScheme) private var scheme
    @Environment(\.dismiss) private var dismiss

    /// Whether the wallet shows local tx history. Bound to the same default the
    /// WalletView reads so toggling here updates the tab.
    @AppStorage(WalletPrefs.showTxHistory) private var showHist = false
    @AppStorage(WalletPrefs.beamCapLifted) private var capLifted = false

    /// Mirrors Android's BeamApi.available — the BEAM section only shows when the
    /// engine build includes the private wallet.
    let beamAvailable: Bool

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                Text("Wallet settings")
                    .font(.system(size: 18, weight: .bold))
                    .foregroundStyle(Hey.ink(scheme))
                Spacer().frame(height: 16)

                // Show transaction history.
                HStack(spacing: 10) {
                    Image(systemName: "doc.text")
                        .font(.system(size: 22))
                        .foregroundStyle(Hey.goldInk(scheme))
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Show transaction history")
                            .font(.system(size: 14, weight: .semibold))
                            .foregroundStyle(Hey.ink(scheme))
                        Text("Your sends + tips (received payments coming soon).")
                            .font(.system(size: 12))
                            .foregroundStyle(Hey.muted(scheme))
                    }
                    Spacer(minLength: 0)
                    Toggle("", isOn: $showHist)
                        .labelsHidden()
                        .tint(Hey.gold)
                }
                .padding(14)
                .glass(14)

                if beamAvailable {
                    Spacer().frame(height: 14)
                    // BEAM private wallet — sync explainer + send safety cap.
                    VStack(alignment: .leading, spacing: 0) {
                        HStack(spacing: 10) {
                            Image(systemName: "shield.fill")
                                .font(.system(size: 22))
                                .foregroundStyle(Hey.goldInk(scheme))
                            Text("BEAM private wallet")
                                .font(.system(size: 14, weight: .semibold))
                                .foregroundStyle(Hey.ink(scheme))
                        }
                        Spacer().frame(height: 4)
                        HStack(alignment: .top, spacing: 6) {
                            Image(systemName: "bolt.fill")
                                .font(.system(size: 14))
                                .foregroundStyle(Hey.goldInk(scheme))
                            Text("Quick sync — light FlyClient verification against official BEAM nodes. Nothing runs on your phone.")
                                .font(.system(size: 12))
                                .foregroundStyle(Hey.muted(scheme))
                        }
                        Spacer().frame(height: 14)
                        Divider().overlay(Hey.glassBorder(scheme))
                        Spacer().frame(height: 12)
                        HStack(alignment: .center, spacing: 8) {
                            VStack(alignment: .leading, spacing: 2) {
                                Text("Lift the send safety cap")
                                    .font(.system(size: 13, weight: .semibold))
                                    .foregroundStyle(Hey.ink(scheme))
                                Text("First sends are limited to \(WalletPrefs.beamSendCap) BEAM. Turn this on only AFTER a successful test send.")
                                    .font(.system(size: 11))
                                    .foregroundStyle(Hey.muted(scheme))
                            }
                            Spacer(minLength: 0)
                            Toggle("", isOn: $capLifted)
                                .labelsHidden()
                                .tint(Hey.gold)
                        }
                    }
                    .padding(14)
                    .glass(14)
                }

                Spacer().frame(height: 20)
            }
            .padding(20)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .scrollContentBackground(.hidden)
        .background(Hey.sheetBg(scheme).ignoresSafeArea())
        .presentationDetents([.medium, .large])
    }
}

/// UserDefaults keys + constants for the wallet (Android keeps these in HeyApi /
/// BeamApi). Shared within the wallet group.
enum WalletPrefs {
    static let showTxHistory = "hey.wallet.showTxHistory"
    static let beamCapLifted = "hey.wallet.beamCapLifted"
    static let essentialsNoteDismissed = "hey.wallet.essentialsNoteDismissed"
    static let beamSendCap = "1"   // BeamApi.SEND_CAP_BEAM display value
}
