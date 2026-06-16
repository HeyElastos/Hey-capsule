import SwiftUI

// Dock-morph Verse menus — Swift port of MainActivity.kt's verse bottom sheets:
//   VerseWorldsSheet  (1352)  pick a world + lighting presets
//   VerseInviteSheet  (1408)  invite a contact into a live visit  → VerseLane.invite → verseSend
//   VerseLibrarySheet (1471)  NFTs you own on ESC → hang one as a painting
//   VerseSashFaqSheet (1275)  Sash's FAQ about Elacity
//   VerseSheetTitle   (1253)  the shared header used by each sheet (private helper)
//
// These are self-contained Views presented from the verse dock. Game commands go
// through VerseLane.shared.postUi(...) (the postUi(cmd) bridge); world "Visit"
// also surfaces `onEnterWorld(world)` so the host can show the game. Invites ride
// the engine verse lane via VerseLane.shared.invite(did) (→ engine.verseSend).
//
// All four are wrapped by the host in a .sheet with sheetBg + an onClose closure.

// MARK: - Shared sheet title

/// Port of VerseSheetTitle (MainActivity.kt:1253) — bold gold-ink title + muted sub.
private struct VerseSheetTitle: View {
    @Environment(\.colorScheme) private var scheme
    let title: String
    let sub: String
    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(title).font(.system(size: 19, weight: .bold)).foregroundStyle(Hey.goldInk(scheme))
            Spacer().frame(height: 3)
            Text(sub).font(.system(size: 13)).foregroundStyle(Hey.muted(scheme))
            Spacer().frame(height: 14)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 22)
    }
}

/// Port of VerseCmdButton (MainActivity.kt:1263) — glass-filled command pill that
/// posts a UI command to the game queue.
private struct VerseCmdButton: View {
    @Environment(\.colorScheme) private var scheme
    let label: String
    let cmd: String
    var body: some View {
        Button { VerseLane.shared.postUi(cmd) } label: {
            Text(label).font(.system(size: 14)).foregroundStyle(Hey.ink(scheme))
                .padding(.vertical, 10).padding(.horizontal, 16)
                .background(Hey.glassFill(scheme), in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                .overlay(
                    RoundedRectangle(cornerRadius: 18, style: .continuous)
                        .strokeBorder(Hey.glassBorder(scheme), lineWidth: 1)
                )
        }
        .buttonStyle(.plain)
    }
}

/// Small gold "action" button used for Visit / Invite / Hang.
private struct VerseGoldButton: View {
    let label: String
    var enabled: Bool = true
    let action: () -> Void
    var body: some View {
        Button(action: action) {
            Text(label).font(.system(size: 13, weight: .semibold)).foregroundStyle(Hey.navy)
                .padding(.vertical, 8).padding(.horizontal, 14)
                .background(Hey.gold.opacity(enabled ? 1 : 0.4), in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
    }
}

// MARK: - Worlds

/// Port of VerseWorldsSheet (MainActivity.kt:1352). Pick a world (My Home / Ela City)
/// and a lighting preset. "Visit" both posts the goto command AND surfaces
/// onEnterWorld so the host can present the game.
struct VerseWorldsSheet: View {
    @Environment(\.colorScheme) private var scheme
    var onClose: () -> Void = {}
    var onEnterWorld: (String) -> Void = { _ in }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            VerseSheetTitle(title: "Worlds", sub: "Spaces you can visit — and one that is truly yours.")

            worldRow(icon: "house.fill", title: "My Home", subtitle: "your world",
                     subtitleColor: Hey.good(scheme), cmd: "goto_home", world: "home")
            Spacer().frame(height: 8)
            worldRow(icon: "building.2.fill", title: "Ela City",
                     subtitle: "futuristic robot city · vendors + mall",
                     subtitleColor: Hey.muted(scheme), cmd: "goto_city", world: "city")

            Spacer().frame(height: 12)
            Text("Lighting").font(.system(size: 13)).foregroundStyle(Hey.muted(scheme))
                .padding(.horizontal, 22)
            Spacer().frame(height: 6)
            HStack(spacing: 8) {
                VerseCmdButton(label: "Day", cmd: "preset_day")
                VerseCmdButton(label: "Sunset", cmd: "preset_sunset")
                VerseCmdButton(label: "Night", cmd: "preset_night")
            }
            .padding(.horizontal, 22)

            Spacer().frame(height: 16)
            Text("Community worlds — visit and buy spaces others created (shops, galleries, malls…) — coming soon.")
                .font(.system(size: 13)).foregroundStyle(Hey.muted(scheme))
                .padding(.horizontal, 22)
            Spacer().frame(height: 28)
        }
    }

    @ViewBuilder
    private func worldRow(icon: String, title: String, subtitle: String,
                          subtitleColor: Color, cmd: String, world: String) -> some View {
        HStack(spacing: 10) {
            Image(systemName: icon).foregroundStyle(Hey.goldInk(scheme))
            VStack(alignment: .leading, spacing: 0) {
                Text(title).font(.system(size: 15, weight: .semibold)).foregroundStyle(Hey.ink(scheme))
                Text(subtitle).font(.system(size: 12)).foregroundStyle(subtitleColor)
            }
            Spacer()
            VerseGoldButton(label: "Visit") {
                VerseLane.shared.postUi(cmd)
                onEnterWorld(world)
            }
        }
        .padding(14)
        .background(Hey.glassFill(scheme), in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 16, style: .continuous)
                .strokeBorder(Hey.glassBorder(scheme), lineWidth: 1)
        )
        .padding(.horizontal, 22)
    }
}

// MARK: - Invite

/// Port of VerseInviteSheet (MainActivity.kt:1408). Lists 1:1 contacts; tapping
/// "Invite" sends a live-visit invite over the verse lane (→ engine.verseSend).
struct VerseInviteSheet: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var scheme
    var onClose: () -> Void = {}

    @State private var contacts: [Chat] = []
    @State private var loaded = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            VerseSheetTitle(title: "Invite a friend",
                            sub: "Live visits — they walk in as their own ELAnaut. Re-invite after a disconnect.")

            if loaded && contacts.isEmpty {
                Text("No contacts yet — add friends in Chat first.")
                    .font(.system(size: 13)).foregroundStyle(Hey.muted(scheme))
                    .padding(.horizontal, 22)
            }

            VStack(alignment: .leading, spacing: 0) {
                ForEach(contacts.prefix(12)) { c in
                    HStack(alignment: .center) {
                        Text(c.name).font(.system(size: 15)).foregroundStyle(Hey.ink(scheme))
                        Spacer()
                        VerseGoldButton(label: "Invite") { VerseLane.shared.invite(c.id) }
                    }
                    .padding(.vertical, 6)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 22)

            Spacer().frame(height: 28)
        }
        .task {
            // Mirrors HeyApi.chats().filter { !it.isGroup }.
            let cs = (try? await store.engine.chats())?.filter { !$0.isGroup } ?? []
            contacts = cs
            loaded = true
        }
    }
}

// MARK: - Library

private struct VerseNft: Identifiable {
    let name: String
    let image: String          // ipfs://… or https://…
    let contract: String
    let id: String
    var key: String { contract + id }
}

/// Port of VerseLibrarySheet (MainActivity.kt:1471). Reads every NFT the wallet owns
/// on ESC (the chain explorer REST API) and lets you "Hang" one as a painting in your
/// home. ipfs:// images resolve through OUR namespace (engine.content); https loads
/// directly.
struct VerseLibrarySheet: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var scheme
    var onClose: () -> Void = {}

    @State private var nfts: [VerseNft]? = nil    // nil = loading

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            VerseSheetTitle(title: "Library",
                            sub: "Everything you own on ESC — hang any NFT as a painting in your home.")

            Group {
                if nfts == nil {
                    HStack(spacing: 10) {
                        ProgressView().controlSize(.small).tint(Hey.gold)
                        Text("reading your wallet on ESC…")
                            .font(.system(size: 13)).foregroundStyle(Hey.muted(scheme))
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 22)
                } else if nfts!.isEmpty {
                    Text("No NFTs found for this wallet on ESC yet — anything you collect on ela.city will appear here.")
                        .font(.system(size: 13)).foregroundStyle(Hey.muted(scheme))
                        .padding(.horizontal, 22)
                } else {
                    ScrollView {
                        VStack(spacing: 0) {
                            ForEach(nfts!.prefix(40)) { nft in
                                row(nft)
                            }
                        }
                    }
                    .frame(maxHeight: 380)
                    .padding(.horizontal, 22)
                }
            }

            Spacer().frame(height: 12)
            Text("Coming soon: placeable .ddrm assets from Elacity (pets, furniture, kitchens…) owned in your namespace.")
                .font(.system(size: 13)).foregroundStyle(Hey.muted(scheme))
                .padding(.horizontal, 22)
            Spacer().frame(height: 28)
        }
        .task { await load() }
    }

    @ViewBuilder
    private func row(_ nft: VerseNft) -> some View {
        HStack(spacing: 10) {
            thumbnail(nft)
            Text(nft.name).font(.system(size: 14)).foregroundStyle(Hey.ink(scheme))
                .lineLimit(1).truncationMode(.tail)
                .frame(maxWidth: .infinity, alignment: .leading)
            VerseGoldButton(label: "Hang", enabled: !nft.image.isEmpty) {
                hang(nft)
            }
        }
        .padding(.vertical, 6)
    }

    @ViewBuilder
    private func thumbnail(_ nft: VerseNft) -> some View {
        let shape = RoundedRectangle(cornerRadius: 10, style: .continuous)
        Group {
            if nft.image.hasPrefix("ipfs://") {
                // ipfs:// resolves through OUR namespace via the content provider.
                ContentImage(cid: Self.ipfsCid(nft.image)) {
                    Rectangle().fill(Hey.glassFill(scheme))
                }
                .scaledToFill()
            } else if !nft.image.isEmpty, let url = URL(string: nft.image) {
                AsyncImage(url: url) { img in
                    img.resizable().scaledToFill()
                } placeholder: {
                    Rectangle().fill(Hey.glassFill(scheme))
                }
            } else {
                Rectangle().fill(Hey.glassFill(scheme))
            }
        }
        .frame(width: 46, height: 46)
        .clipShape(shape)
    }

    // ── data ────────────────────────────────────────────────────────────────
    private func load() async {
        guard let addr = await store.engine.walletAddress(), !addr.isEmpty else {
            nfts = []; return
        }
        nfts = await Self.fetchEscNfts(addr)
    }

    /// Writes the chosen NFT image to a temp file and tells the game to hang it.
    /// ipfs:// bytes come from the in-process content store; https loads directly.
    private func hang(_ nft: VerseNft) {
        Task {
            let bytes: Data
            if nft.image.hasPrefix("ipfs://") {
                bytes = await store.engine.content(cid: Self.ipfsCid(nft.image)) ?? Data()
            } else if let url = URL(string: nft.image) {
                bytes = (try? await URLSession.shared.data(from: url).0) ?? Data()
            } else {
                bytes = Data()
            }
            guard !bytes.isEmpty else { return }
            let file = FileManager.default.temporaryDirectory
                .appendingPathComponent("verse_nft_\(nft.key.hashValue).img")
            guard (try? bytes.write(to: file)) != nil else { return }
            VerseLane.shared.postUi("hang:\(file.path)")
        }
    }

    /// `ipfs://<cid>/...` or `ipfs/<cid>/...` → bare cid.
    private static func ipfsCid(_ raw: String) -> String {
        var s = raw
        if s.hasPrefix("ipfs://") { s.removeFirst("ipfs://".count) }
        if s.hasPrefix("ipfs/") { s.removeFirst("ipfs/".count) }
        return String(s.prefix { $0 != "/" })
    }

    /// Every NFT the wallet owns on ESC — all ERC-721/1155 contracts (everything
    /// traded on ela.city), via the chain explorer's public REST API.
    private static func fetchEscNfts(_ addr: String) async -> [VerseNft] {
        let api = "https://esc.elastos.io/api/v2/addresses/\(addr)/nft?type=ERC-721%2CERC-1155"
        guard let url = URL(string: api) else { return [] }
        var req = URLRequest(url: url, timeoutInterval: 8)
        req.setValue("application/json", forHTTPHeaderField: "Accept")
        guard let (data, _) = try? await URLSession.shared.data(for: req),
              let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let items = root["items"] as? [[String: Any]] else { return [] }

        var out: [VerseNft] = []
        for o in items {
            let meta = o["metadata"] as? [String: Any]
            let token = o["token"] as? [String: Any]
            let idStr = stringy(o["id"])
            var name = stringy(meta?["name"])
            if name.isEmpty { name = stringy(token?["name"]) }
            if name.isEmpty { name = "NFT #\(idStr)" }
            var img = stringy(o["image_url"])
            if img.isEmpty { img = stringy(meta?["image"]) }
            out.append(VerseNft(name: name, image: img,
                                contract: stringy(token?["address"]), id: idStr))
        }
        return out
    }

    private static func stringy(_ v: Any?) -> String {
        switch v {
        case let s as String: return s
        case let n as NSNumber: return n.stringValue
        default: return ""
        }
    }
}

// MARK: - Sash FAQ

/// Port of VerseSashFaqSheet (MainActivity.kt:1275). Sash (the creator of Elacity)
/// answers a few questions about Elacity — opened by tapping him in Ela City.
struct VerseSashFaqSheet: View {
    @Environment(\.colorScheme) private var scheme
    var onClose: () -> Void = {}

    private let faq: [(String, String)] = [
        ("What is Elacity?",
         "The World Computer Marketplace — a place where digital things are truly owned, traded and enjoyed by people, not platforms. It runs on Elastos."),
        ("What does \"truly owned\" mean?",
         "Your assets live in your own namespace on your own devices, secured by the chain. No platform can take them away or lock you in."),
        ("What is dDRM?",
         "Decentralized DRM: media travels encrypted, and owning the access token releases the key. Files stay yours and play anywhere — no central server."),
        ("What can I buy on ela.city?",
         "Digital assets — art, video, 3D models. Soon: wearables for your robot, furniture and assets for your home and worlds here in the Verse."),
        ("What is PC2?",
         "Your Personal Cloud Computer — your own corner of the world computer that runs your spaces and serves your content."),
        ("Why a city?",
         "Because a marketplace should feel like a place. Walk around, meet people, window-shop — the mall opens its stores soon!"),
    ]

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ZStack(alignment: .topTrailing) {
                VerseSheetTitle(title: "Sash · about Elacity",
                                sub: "the creator of Elacity answers a few questions")
                Button(action: onClose) {
                    Image(systemName: "xmark").font(.system(size: 16, weight: .semibold))
                        .foregroundStyle(Hey.muted(scheme))
                }
                .padding(.trailing, 10)
            }

            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(Array(faq.enumerated()), id: \.offset) { _, qa in
                        Text(qa.0).font(.system(size: 14, weight: .semibold)).foregroundStyle(Hey.ink(scheme))
                        Spacer().frame(height: 3)
                        Text(qa.1).font(.system(size: 13)).foregroundStyle(Hey.muted(scheme))
                            .lineSpacing(18 - 13)
                        Spacer().frame(height: 12)
                    }
                    Text("more at elacitylabs.com")
                        .font(.system(size: 12)).foregroundStyle(Hey.goldInk(scheme))
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 22)
            }
            .frame(maxHeight: 420)

            Spacer().frame(height: 28)
        }
    }
}
