import SwiftUI
import UIKit

// Port of WalletScreen (MainActivity.kt:3101-3307). One BIP39 seed → all chains
// (DID + ELA mainchain + ESC + ERC-20 tokens [+ BEAM]). Chains come from the Rust
// registry (walletChains); the Elastos mainchain (E…) is its own card; BEAM appears
// when the engine build includes it. Swipe the chain cards, then Send / Receive.
//
// Send sheets live in the wallet-send group — this view bubbles a send request up via
// closures so the orchestrator presents the right sheet. Receive + tokens + settings
// are owned here.
struct WalletView: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var scheme

    /// Cross-group send navigation (wallet-send group owns the sheets). Defaults are
    /// no-ops so WalletView() compiles + RootView can wire them later.
    var onSendEvm: (_ chain: WalletChain, _ token: TokenBal?) -> Void = { _, _ in }
    var onSendEla: () -> Void = { }
    var onSendBeam: () -> Void = { }

    @State private var chains: [WalletChain] = []
    @State private var evmAddr: String?
    @State private var elaAddr: String?
    @State private var beamAddr: String?
    @State private var did: String?
    @State private var bal: [String: String] = [:]      // chain key → decimal balance
    @State private var loading = true
    @State private var page = 0
    @State private var refreshKey = 0

    // Sheets owned by this view.
    @State private var receiveChain: WalletChain?
    @State private var tokensChain: WalletChain?
    @State private var showSettings = false

    @AppStorage(WalletPrefs.showTxHistory) private var showTxHistory = false
    @AppStorage(WalletPrefs.essentialsNoteDismissed) private var essentialsDismissed = false
    @State private var history: [TxRecord] = []

    private var active: WalletChain? {
        guard !chains.isEmpty else { return nil }
        return chains[min(max(page, 0), chains.count - 1)]
    }
    private func addr(for c: WalletChain) -> String? {
        c.evm ? evmAddr : (c.key == "beam" ? beamAddr : elaAddr)
    }
    private var activeAddr: String? { active.flatMap(addr(for:)) }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                header
                Spacer().frame(height: 12)
                didCard
                chainPager
                dots
                Spacer().frame(height: 16)
                sendReceive
                Spacer().frame(height: 6)
                refreshButton
                Spacer().frame(height: 10)
                essentialsNote
                txHistorySection
            }
            .padding(.init(top: 12, leading: 20, bottom: 110, trailing: 20))
        }
        .scrollContentBackground(.hidden)
        .background(FrostBackground())
        .task(id: refreshKey) { await reload() }
        .sheet(item: $receiveChain) { c in
            ReceiveSheet(address: addr(for: c) ?? "", chainTitle: c.title, chainSub: c.sub, symbol: c.symbol)
        }
        .sheet(item: $tokensChain) { c in
            TokenSheet(chain: c) { token in
                // Tap a token → close tokens, open the send sheet (wallet-send group).
                tokensChain = nil
                onSendEvm(c, token)
            }
        }
        .sheet(isPresented: $showSettings) {
            WalletSettingsSheet(beamAvailable: store.engine.beamAvailable)
        }
        .onChange(of: showTxHistory) { _ in Task { await loadHistory() } }
    }

    // MARK: header

    private var header: some View {
        HStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 2) {
                Text("Wallet").font(.system(size: 22, weight: .bold)).foregroundStyle(Hey.ink(scheme))
                Text("Elastos identity + chains").font(.system(size: 12)).foregroundStyle(Hey.muted(scheme))
            }
            Spacer(minLength: 0)
            Button { showSettings = true } label: {
                Image(systemName: "gearshape.fill")
                    .font(.system(size: 20))
                    .foregroundStyle(Hey.muted(scheme))
            }
        }
    }

    // MARK: DID card (the wallet's umbrella identity / EID)

    @ViewBuilder private var didCard: some View {
        if let d = did {
            VStack(alignment: .leading, spacing: 0) {
                HStack(spacing: 0) {
                    Image(systemName: "person.text.rectangle")
                        .font(.system(size: 20)).foregroundStyle(Hey.goldInk(scheme))
                    Spacer().frame(width: 8)
                    Text("Your Elastos DID")
                        .font(.system(size: 14, weight: .semibold)).foregroundStyle(Hey.ink(scheme))
                    Spacer(minLength: 0)
                    Text("EID").font(.system(size: 11)).foregroundStyle(Hey.muted(scheme))
                }
                Spacer().frame(height: 8)
                Button { UIPasteboard.general.string = d } label: {
                    HStack(spacing: 8) {
                        Text(prettyDid(d))
                            .font(HeyFont.mono(12)).foregroundStyle(Hey.ink(scheme))
                            .frame(maxWidth: .infinity, alignment: .leading)
                        Image(systemName: "doc.on.doc")
                            .font(.system(size: 14)).foregroundStyle(Hey.muted(scheme))
                    }
                    .padding(.horizontal, 10).padding(.vertical, 8)
                    .background(Color.black.opacity(0.10),
                                in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                }
                .buttonStyle(.plain)
            }
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
            .glass(18)
            Spacer().frame(height: 14)
        }
    }

    private func prettyDid(_ d: String) -> String {
        let body = d.replacingOccurrences(of: "did:elastos:", with: "")
        return "did:elastos:\(body.prefix(8))…\(body.suffix(6))"
    }

    // MARK: chain pager + dots

    private var chainPager: some View {
        TabView(selection: $page) {
            ForEach(Array(chains.enumerated()), id: \.element.id) { i, c in
                ChainCard(
                    chain: c,
                    address: addr(for: c),
                    balance: bal[c.key],
                    loading: loading,
                    onTap: {
                        if c.evm { tokensChain = c }
                        else if c.key == "beam" { onSendBeam() }   // BEAM assets sheet → send (no assets sheet on iOS yet)
                    },
                    onCopy: { UIPasteboard.general.string = $0 }
                )
                .padding(.horizontal, 2)
                .tag(i)
            }
        }
        .tabViewStyle(.page(indexDisplayMode: .never))
        .frame(height: chains.isEmpty ? 0 : 250)
    }

    private var dots: some View {
        HStack(spacing: 0) {
            Spacer(minLength: 0)
            ForEach(Array(chains.enumerated()), id: \.element.id) { i, _ in
                Circle()
                    .fill(page == i ? Hey.goldInk(scheme) : Hey.muted(scheme).opacity(0.4))
                    .frame(width: page == i ? 9 : 7, height: page == i ? 9 : 7)
                    .padding(.horizontal, 4)
            }
            Spacer(minLength: 0)
        }
        .padding(.top, 12)
    }

    // MARK: send / receive

    private var sendReceive: some View {
        HStack(spacing: 12) {
            Button {
                guard let a = active else { return }
                if a.evm { onSendEvm(a, nil) }
                else if a.key == "beam" { onSendBeam() }
                else { onSendEla() }
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: "paperplane.fill").font(.system(size: 18))
                    Text("Send").font(.system(size: 15, weight: .bold))
                }
                .frame(maxWidth: .infinity).padding(.vertical, 12)
                .foregroundStyle(Hey.navy)
                .background(Hey.gold, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
            }
            .disabled(activeAddr == nil)
            .opacity(activeAddr == nil ? 0.5 : 1)

            Button {
                receiveChain = active
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: "qrcode").font(.system(size: 18))
                    Text("Receive").font(.system(size: 15))
                }
                .frame(maxWidth: .infinity).padding(.vertical, 12)
                .foregroundStyle(Hey.ink(scheme))
                .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .strokeBorder(Hey.glassBorder(scheme), lineWidth: 1))
            }
            .disabled(activeAddr == nil)
            .opacity(activeAddr == nil ? 0.5 : 1)
        }
    }

    private var refreshButton: some View {
        Button { refreshKey += 1 } label: {
            HStack(spacing: 4) {
                Image(systemName: "arrow.clockwise").font(.system(size: 16))
                Text("Refresh").font(.system(size: 13))
            }
            .foregroundStyle(Hey.muted(scheme))
        }
        .buttonStyle(.plain)
    }

    // MARK: Essentials compatibility note (dismissable, remembered)

    @ViewBuilder private var essentialsNote: some View {
        if !essentialsDismissed {
            HStack(alignment: .top, spacing: 10) {
                Image(systemName: "shield.fill")
                    .font(.system(size: 20)).foregroundStyle(Hey.goldInk(scheme))
                VStack(alignment: .leading, spacing: 3) {
                    Text("Same wallets as Elastos Essentials")
                        .font(.system(size: 13, weight: .semibold)).foregroundStyle(Hey.ink(scheme))
                    Text("Every address here is derived from your recovery phrase. Import that phrase into official Elastos Essentials and you'll see the same DID + wallets. Your keys never leave the phone.")
                        .font(.system(size: 12)).foregroundStyle(Hey.muted(scheme))
                }
                Button { essentialsDismissed = true } label: {
                    Image(systemName: "xmark")
                        .font(.system(size: 16)).foregroundStyle(Hey.muted(scheme))
                }
                .buttonStyle(.plain)
            }
            .padding(14)
            .frame(maxWidth: .infinity, alignment: .leading)
            .glass(16)
            Spacer().frame(height: 10)
        }
    }

    // MARK: transaction history (toggle in the gear)

    @ViewBuilder private var txHistorySection: some View {
        if showTxHistory {
            Spacer().frame(height: 18)
            Text("Recent activity")
                .font(.system(size: 14, weight: .semibold)).foregroundStyle(Hey.ink(scheme))
            Spacer().frame(height: 8)
            if history.isEmpty {
                Text("Your sends and tips will show here.")
                    .font(.system(size: 12)).foregroundStyle(Hey.muted(scheme))
            } else {
                ForEach(history.prefix(25)) { TxRow(tx: $0) }
            }
        }
    }

    // MARK: data

    private func reload() async {
        loading = true
        if chains.isEmpty { chains = await buildChains() }

        did = await store.engine.elastosDid()
        evmAddr = await store.engine.walletAddress()
        elaAddr = await store.engine.elaAddress()
        beamAddr = store.engine.beamAvailable ? await store.engine.beamAddress() : nil

        var m: [String: String] = [:]
        for c in chains where c.evm {
            if let b = await store.engine.walletBalance(chain: c.key)?.balance { m[c.key] = b }
        }
        if let ela = await store.engine.elaBalance() { m["ela"] = ela }
        if store.engine.beamAvailable, let bb = await store.engine.beamBalance()?.beam { m["beam"] = bb }
        bal = m

        // Publish receive addresses so followers can tip by identity ("just works").
        if !UserDefaults.standard.bool(forKey: "hey.wallet.tipsPublished") {
            if await store.engine.publishTipAddresses() {
                UserDefaults.standard.set(true, forKey: "hey.wallet.tipsPublished")
            }
        }

        await loadHistory()
        loading = false
    }

    private func loadHistory() async {
        guard showTxHistory else { return }
        history = await store.engine.txHistory()
    }

    /// Build the chain stack from the Rust registry. EVM chains share one 0x address;
    /// the Elastos mainchain + (optional) BEAM are appended. Mirrors WalletScreen:3116.
    private func buildChains() async -> [WalletChain] {
        let evm = await store.engine.walletChains()
        let evmList: [WalletChain] = evm.isEmpty
            ? [WalletChain(key: "esc", title: "Elastos Smart Chain", sub: "ELA · EVM", evm: true, symbol: "ELA")]
            : evm.map { WalletChain(key: $0.key, title: $0.name, sub: "\($0.symbol) · EVM", evm: true, symbol: $0.symbol) }
        var extra: [WalletChain] = [
            WalletChain(key: "ela", title: "Elastos Mainchain", sub: "ELA · mainchain", evm: false, symbol: "ELA")
        ]
        if store.engine.beamAvailable {
            extra.append(WalletChain(key: "beam", title: "BEAM", sub: "Mimblewimble · private", evm: false, symbol: "BEAM"))
        }
        return evmList + extra
    }
}
