import SwiftUI
import CoreImage.CIFilterBuiltins
import UIKit

// Port of ReceiveSheet (MainActivity.kt:3675-3705): a QR of the receive address +
// the full address + Share / Copy. Network warning copy matches Android 1:1.
struct ReceiveSheet: View {
    @Environment(\.colorScheme) private var scheme
    @Environment(\.dismiss) private var dismiss

    let address: String
    let chainTitle: String
    let chainSub: String
    let symbol: String

    @State private var qr: UIImage?

    var body: some View {
        ScrollView {
            VStack(spacing: 0) {
                Text("Receive \(symbol)")
                    .font(.system(size: 18, weight: .bold))
                    .foregroundStyle(Hey.ink(scheme))
                Spacer().frame(height: 4)
                Text("Scan or copy to receive \(symbol) on \(chainTitle) (\(chainSub)). Only send \(symbol) on this network to this address.")
                    .font(.system(size: 12))
                    .foregroundStyle(Hey.muted(scheme))
                    .multilineTextAlignment(.center)
                Spacer().frame(height: 16)

                // White QR plate (the QR is dark-on-white, so a white backing always reads).
                ZStack {
                    if let qr {
                        Image(uiImage: qr)
                            .resizable()
                            .interpolation(.none)
                            .scaledToFit()
                    } else {
                        ProgressView().tint(Hey.navy)
                    }
                }
                .padding(10)
                .frame(maxWidth: .infinity)
                .aspectRatio(1, contentMode: .fit)
                .background(Color.white, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
                .frame(maxWidth: .infinity)
                .scaleEffect(0.78, anchor: .center)

                Spacer().frame(height: 14)
                Text(address)
                    .font(HeyFont.mono(13))
                    .foregroundStyle(Hey.ink(scheme))
                    .multilineTextAlignment(.center)
                Spacer().frame(height: 16)

                HStack(spacing: 10) {
                    ShareLink(item: address) {
                        HStack(spacing: 6) {
                            Image(systemName: "square.and.arrow.up").font(.system(size: 18))
                            Text("Share").font(.system(size: 15, weight: .bold))
                        }
                        .padding(.horizontal, 18).padding(.vertical, 11)
                        .foregroundStyle(Hey.navy)
                        .background(Hey.gold, in: Capsule())
                    }
                    Button {
                        UIPasteboard.general.string = address
                    } label: {
                        HStack(spacing: 6) {
                            Image(systemName: "doc.on.doc").font(.system(size: 18))
                            Text("Copy").font(.system(size: 15))
                        }
                        .padding(.horizontal, 18).padding(.vertical, 11)
                        .foregroundStyle(Hey.ink(scheme))
                        .overlay(Capsule().strokeBorder(Hey.glassBorder(scheme), lineWidth: 1))
                    }
                }
                Spacer().frame(height: 24)
            }
            .padding(20)
            .frame(maxWidth: .infinity)
        }
        .scrollContentBackground(.hidden)
        .background(Hey.sheetBg(scheme).ignoresSafeArea())
        .presentationDetents([.medium, .large])
        .task(id: address) { qr = Self.makeQR(address) }
    }

    /// Render `text` to a crisp black-on-white QR UIImage (CIFilter, then upscaled
    /// nearest-neighbour). Android uses ZXing; CoreImage is the iOS equivalent.
    private static func makeQR(_ text: String) -> UIImage? {
        let ctx = CIContext()
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(text.utf8)
        filter.correctionLevel = "M"
        guard let output = filter.outputImage else { return nil }
        let scale: CGFloat = 12
        let scaled = output.transformed(by: CGAffineTransform(scaleX: scale, y: scale))
        guard let cg = ctx.createCGImage(scaled, from: scaled.extent) else { return nil }
        return UIImage(cgImage: cg)
    }
}
