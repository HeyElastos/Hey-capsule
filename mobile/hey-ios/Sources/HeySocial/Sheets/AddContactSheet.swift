import SwiftUI
import CoreImage
import CoreImage.CIFilterBuiltins
import UIKit

// "New chat" sheet — 1:1 port of AddContactSheet (MainActivity.kt:5301-5422).
// Three ways to start a conversation:
//   1. Tap someone you already follow (they're DM-capable — their link carried PQ keys).
//   2. Paste a Hey friend link (hey:follow:…) or an invite (hey-invite:…) → connect.
//   3. Share YOUR invite: friendLink() as a copyable/shareable link + a scannable QR.
//
// Engine parity with Android: acceptInvite(token:) returns the new contact's `did`, so an
// accepted hey-invite opens the chat immediately (onStartChat(did)). follow(_:) returns Void
// (following someone doesn't open a thread), so a hey:follow: friend link follows then closes
// and the new contact surfaces in the chat list.
struct AddContactSheet: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var scheme

    var onClose: () -> Void = {}
    var onStartChat: (String) -> Void = { _ in }

    @State private var link = ""                     // my friendLink()
    @State private var qr: UIImage? = nil
    @State private var input = ""                    // pasted link / invite
    @State private var status = ""
    @State private var following: [Follow] = []
    @State private var showShare = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                Text("New chat")
                    .font(.system(size: 18, weight: .bold))
                    .foregroundStyle(Hey.ink(scheme))
                Text("Message someone you follow, paste their Hey friend link, or share your invite.")
                    .font(.system(size: 13))
                    .foregroundStyle(Hey.muted(scheme))
                    .padding(.top, 4)

                // ── People you follow ──
                if !following.isEmpty {
                    sectionLabel("People you follow").padding(.top, 16)
                    ForEach(following) { f in
                        PersonRow(did: f.did, name: Profile.short(f.did)) {
                            startWith(f.did)
                        }
                    }
                    Divider().background(Hey.glassBorder(scheme)).padding(.top, 8)
                }

                // ── Add by link or invite ──
                sectionLabel("Add by link or invite").padding(.top, 16)

                TextField("", text: $input, prompt:
                    Text("Paste a friend link or invite…").foregroundColor(Hey.muted(scheme)))
                    .font(.system(size: 13))
                    .foregroundStyle(Hey.ink(scheme))
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .padding(12)
                    .glass(12)
                    .padding(.top, 10)

                if input.count > 24 {
                    Text("✓ Link ready (\(input.count) chars)")
                        .font(.system(size: 11))
                        .foregroundStyle(Hey.good(scheme))
                        .padding(.top, 4)
                }

                HStack(spacing: 12) {
                    Button {
                        if let s = UIPasteboard.general.string { input = HeyQR.fromScan(s) }
                    } label: {
                        Label("Paste", systemImage: "doc.on.clipboard")
                            .font(.system(size: 14))
                            .foregroundStyle(Hey.ink(scheme))
                            .padding(.horizontal, 14).padding(.vertical, 9)
                            .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(Hey.glassBorder(scheme)))
                    }
                    .buttonStyle(.plain)

                    Button { submit() } label: {
                        Text("Start chat")
                            .font(.system(size: 14, weight: .bold))
                            .foregroundStyle(Hey.navy)
                            .padding(.horizontal, 16).padding(.vertical, 9)
                            .background(Hey.gold, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                    }
                    .buttonStyle(.plain)
                }
                .padding(.top, 12)

                if !status.isEmpty {
                    Text(status)
                        .font(.system(size: 13))
                        .foregroundStyle(Hey.muted(scheme))
                        .padding(.top, 10)
                }

                Divider().background(Hey.glassBorder(scheme)).padding(.top, 20)

                // ── Share your invite ──
                sectionLabel("Or share your invite").padding(.top, 16)
                Text("Best: Share the link. The QR is dense (it carries your encryption key) — scan close, in good light.")
                    .font(.system(size: 11))
                    .foregroundStyle(Hey.muted(scheme))
                    .padding(.top, 4)

                ZStack {
                    RoundedRectangle(cornerRadius: 16, style: .continuous).fill(.white)
                    if let qr {
                        Image(uiImage: qr).resizable().interpolation(.none).scaledToFit().padding(10)
                    } else if link.isEmpty {
                        ProgressView().tint(Hey.navy)
                    } else {
                        Text("Use Share / Copy below").font(.system(size: 13)).foregroundStyle(Hey.navy)
                    }
                }
                .aspectRatio(1, contentMode: .fit)
                .padding(.top, 10)

                HStack(spacing: 10) {
                    Spacer()
                    Button { showShare = true } label: {
                        Label("Share link", systemImage: "square.and.arrow.up")
                            .font(.system(size: 14, weight: .bold))
                            .foregroundStyle(Hey.navy)
                            .padding(.horizontal, 16).padding(.vertical, 9)
                            .background(Hey.gold, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                    }
                    .buttonStyle(.plain)
                    .disabled(link.isEmpty)

                    Button {
                        UIPasteboard.general.string = link
                    } label: {
                        Label("Copy", systemImage: "doc.on.doc")
                            .font(.system(size: 14))
                            .foregroundStyle(Hey.ink(scheme))
                            .padding(.horizontal, 14).padding(.vertical, 9)
                            .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(Hey.glassBorder(scheme)))
                    }
                    .buttonStyle(.plain)
                    .disabled(link.isEmpty)
                    Spacer()
                }
                .padding(.top, 10)
            }
            .padding(20)
            .padding(.bottom, 24)
        }
        .scrollContentBackground(.hidden)
        .background(Hey.sheetBg(scheme).ignoresSafeArea())
        .presentationDetents([.large])
        .presentationDragIndicator(.visible)
        .sheet(isPresented: $showShare) { ShareSheet(items: [link]) }
        .task { await load() }
    }

    private func sectionLabel(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 12, weight: .semibold))
            .foregroundStyle(Hey.muted(scheme))
            .padding(.bottom, 6)
    }

    private func load() async {
        // Share the SAME compact friend-link QR everywhere — it's DM-capable and scannable.
        let l = await store.engine.friendLink()
        link = l
        if !l.isEmpty { qr = HeyQR.image(HeyQR.toQr(l)) }
        following = (try? await store.engine.following()) ?? []
    }

    private func startWith(_ did: String) {
        status = "Starting…"
        Task {
            try? await store.engine.startChat(did: did)
            onStartChat(did)
        }
    }

    private func submit() {
        // A scanned/pasted QR may be our tagged compact form — normalize it first.
        let v = HeyQR.fromScan(input.trimmingCharacters(in: .whitespacesAndNewlines))
        guard !v.isEmpty else { status = "Paste a friend link or invite"; return }
        status = "Connecting…"
        Task {
            if v.hasPrefix("hey:follow:") {
                // The friend link carries the PQ keys → follow bootstraps a DM.
                do {
                    try await store.engine.follow(v)
                    // TODO(unresolved): follow(_:) returns Void — no `did` to open the
                    // chat with. Android's HeyApi.follow returns JSON {did}. Until the
                    // contract exposes that, close and let the new chat surface in the list.
                    status = ""
                    onClose()
                } catch {
                    status = "Failed: \(error.localizedDescription)"
                }
            } else if v.hasPrefix("hey-invite:") {
                do {
                    let did = try await store.engine.acceptInvite(token: v)
                    status = ""
                    // Open the new chat straight away when the engine surfaced the did;
                    // otherwise close and let the contact appear in the list.
                    if !did.isEmpty { onStartChat(did) } else { onClose() }
                } catch {
                    status = "Failed: \(error.localizedDescription)"
                }
            } else if v.hasPrefix("did:") {
                status = "That's a DID — paste a Hey friend link instead."
            } else {
                status = "Unrecognized — paste a Hey friend link or scan a Hey QR."
            }
        }
    }
}

// QR helpers — port of QrLink (MainActivity.kt:2841-2880) + qrBitmap (2885-2910).
// The compact form re-encodes the friend link's URL-safe base64 payload as Crockford-ish
// base32 behind a "HEYF" tag so the QR has the fewest, largest, most-scannable modules.
enum HeyQR {
    private static let alphabet = Array("ABCDEFGHIJKLMNOPQRSTUVWXYZ234567")
    private static let tag = "HEYF"

    /// friend link → compact alphanumeric QR payload (falls back to the link).
    static func toQr(_ link: String) -> String {
        guard let range = link.range(of: "hey:follow:") else { return link }
        let b64 = String(link[range.upperBound...])
        guard !b64.isEmpty, let raw = base64urlDecode(b64) else { return link }
        return tag + base32enc(raw)
    }

    /// scanned/pasted text → original friend link if it's our tagged QR, else trimmed input.
    static func fromScan(_ s: String) -> String {
        let t = s.trimmingCharacters(in: .whitespacesAndNewlines)
        guard t.hasPrefix(tag) else { return t }
        guard let raw = base32dec(String(t.dropFirst(tag.count))) else { return t }
        return "hey:follow:" + base64urlEncode(raw)
    }

    /// Render `text` to a black-on-white QR UIImage (returns nil on failure → copy-only).
    static func image(_ text: String) -> UIImage? {
        guard !text.isEmpty else { return nil }
        let ctx = CIContext()
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(text.utf8)
        // Lowest EC = fewest modules → largest cells for the ~1KB PQ-key payload.
        filter.correctionLevel = "L"
        guard let output = filter.outputImage else { return nil }
        let scaled = output.transformed(by: CGAffineTransform(scaleX: 12, y: 12))
        guard let cg = ctx.createCGImage(scaled, from: scaled.extent) else { return nil }
        return UIImage(cgImage: cg)
    }

    // MARK: base32 (matches QrLink.b32enc/b32dec)
    private static func base32enc(_ data: Data) -> String {
        var out = ""
        var buf = 0, bits = 0
        for b in data {
            buf = (buf << 8) | Int(b); bits += 8
            while bits >= 5 { bits -= 5; out.append(alphabet[(buf >> bits) & 0x1f]) }
        }
        if bits > 0 { out.append(alphabet[(buf << (5 - bits)) & 0x1f]) }
        return out
    }
    private static func base32dec(_ s: String) -> Data? {
        var out = Data()
        var buf = 0, bits = 0
        for c in s {
            guard let v = alphabet.firstIndex(of: c) else { continue }
            buf = (buf << 5) | v; bits += 5
            if bits >= 8 { bits -= 8; out.append(UInt8((buf >> bits) & 0xff)) }
        }
        return out
    }

    // MARK: URL-safe base64, no padding (Android Base64.URL_SAFE | NO_PADDING | NO_WRAP)
    private static func base64urlEncode(_ data: Data) -> String {
        data.base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }
    private static func base64urlDecode(_ s: String) -> Data? {
        var b = s.replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
        while b.count % 4 != 0 { b.append("=") }
        return Data(base64Encoded: b)
    }
}

// UIActivityViewController bridge so "Share link" opens the native share sheet.
// File-private (MyQrSheet declares its own private ShareSheet — no module clash).
private struct ShareSheet: UIViewControllerRepresentable {
    let items: [Any]
    func makeUIViewController(context: Context) -> UIActivityViewController {
        UIActivityViewController(activityItems: items, applicationActivities: nil)
    }
    func updateUIViewController(_ vc: UIActivityViewController, context: Context) {}
}
