import SwiftUI
import CoreImage
import CoreImage.CIFilterBuiltins
import UIKit

/// "Add me on Hey" — shows my invite link as a big scannable QR plus Share / Copy
/// (port of MyQrSheet, MainActivity.kt:2721-2758). The link comes from
/// engine.friendLink(); a bare DID can't open a private channel, so we share the
/// invite link/QR, never the raw DID.
struct MyQrSheet: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var scheme
    @Environment(\.dismiss) private var dismiss

    let did: String

    @State private var link = ""
    @State private var qr: UIImage?
    @State private var copied = false
    @State private var sharing = false

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 14) {
                    Text("Add me on Hey")
                        .font(HeyFont.subtitle.weight(.bold))
                        .foregroundStyle(Hey.ink(scheme))
                        .padding(.top, 4)
                    Text("Best: tap Share and send the link. Or scan the QR up close in good light.")
                        .font(HeyFont.caption)
                        .foregroundStyle(Hey.muted(scheme))
                        .multilineTextAlignment(.center)

                    // As large as the sheet allows → biggest QR cells → most scannable.
                    ZStack {
                        RoundedRectangle(cornerRadius: 16, style: .continuous).fill(.white)
                        if let qr {
                            Image(uiImage: qr)
                                .interpolation(.none)
                                .resizable()
                                .scaledToFit()
                                .padding(10)
                        } else if link.isEmpty {
                            ProgressView().tint(Hey.navy)
                        } else {
                            Text("Use Share / Copy below").foregroundStyle(Hey.navy)
                        }
                    }
                    .aspectRatio(1, contentMode: .fit)
                    .frame(maxWidth: .infinity)

                    HStack(spacing: 10) {
                        Button {
                            sharing = true
                        } label: {
                            Label("Share link", systemImage: "square.and.arrow.up")
                                .font(HeyFont.label.weight(.bold))
                                .frame(maxWidth: .infinity, minHeight: 44)
                        }
                        .foregroundStyle(Hey.navy)
                        .background(Hey.gold, in: RoundedRectangle(cornerRadius: HeyRadius.attachment, style: .continuous))
                        .disabled(link.isEmpty)

                        Button {
                            UIPasteboard.general.string = link
                            withAnimation { copied = true }
                            DispatchQueue.main.asyncAfter(deadline: .now() + 1.4) {
                                withAnimation { copied = false }
                            }
                        } label: {
                            Label(copied ? "Copied" : "Copy", systemImage: copied ? "checkmark" : "doc.on.doc")
                                .font(HeyFont.label)
                                .frame(maxWidth: .infinity, minHeight: 44)
                        }
                        .foregroundStyle(Hey.ink(scheme))
                        .overlay(
                            RoundedRectangle(cornerRadius: HeyRadius.attachment, style: .continuous)
                                .strokeBorder(Hey.glassBorder(scheme), lineWidth: 1)
                        )
                        .disabled(link.isEmpty)
                    }

                    Spacer(minLength: 8)
                }
                .padding(20)
            }
            .scrollContentBackground(.hidden)
            .background(Hey.sheetBg(scheme).ignoresSafeArea())
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Done") { dismiss() }.tint(Hey.muted(scheme))
                }
            }
        }
        .presentationDetents([.large])
        .sheet(isPresented: $sharing) {
            if !link.isEmpty { ShareSheet(items: [link]) }
        }
        .task {
            link = await store.engine.friendLink()
            qr = Self.makeQR(link)
        }
    }

    /// Generate a black-on-white QR for `text` (port of qrBitmap / QrLink.toQr).
    static func makeQR(_ text: String) -> UIImage? {
        guard !text.isEmpty else { return nil }
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(text.utf8)
        filter.correctionLevel = "M"
        guard let output = filter.outputImage else { return nil }
        // Scale the tiny generated image up so it stays crisp when rendered large.
        let scaled = output.transformed(by: CGAffineTransform(scaleX: 16, y: 16))
        let context = CIContext()
        guard let cg = context.createCGImage(scaled, from: scaled.extent) else { return nil }
        return UIImage(cgImage: cg)
    }
}

// UIActivityViewController bridge so "Share link" opens the native share sheet.
// File-private (AddContactSheet declares its own private ShareSheet — no clash).
private struct ShareSheet: UIViewControllerRepresentable {
    let items: [Any]
    func makeUIViewController(context: Context) -> UIActivityViewController {
        UIActivityViewController(activityItems: items, applicationActivities: nil)
    }
    func updateUIViewController(_ vc: UIActivityViewController, context: Context) {}
}
