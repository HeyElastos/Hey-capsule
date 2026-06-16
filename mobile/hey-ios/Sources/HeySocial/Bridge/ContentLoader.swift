import SwiftUI
import UIKit

// iOS equivalent of Android's WebSpaceFetcher (the Coil fetcher for
// `localhost://WebSpaces/hey/<cid>`). Media is addressed by NAMESPACE, never by
// network: a `hey-content://<cid>` URL is resolved to bytes by a custom URLProtocol
// that calls the IN-PROCESS content provider (HeyEngine.content → hey_content_bytes).
// No 127.0.0.1, no gateway, no IP/port — the runtime keeps the network hidden.

/// Resolves `hey-content://<cid>` by asking the engine for the content bytes.
final class ContentURLProtocol: URLProtocol {
    static let scheme = "hey-content"
    private var task: Task<Void, Never>?

    override class func canInit(with request: URLRequest) -> Bool { request.url?.scheme == scheme }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        guard let url = request.url else {
            client?.urlProtocol(self, didFailWithError: URLError(.badURL)); return
        }
        // hey-content://<cid>  → host carries the cid (lastPathComponent as a fallback).
        let cid = (url.host?.isEmpty == false ? url.host! : url.lastPathComponent)
        task = Task { [weak self] in
            guard let self else { return }
            let data = await HeyEngineFactory.live.content(cid: cid)
            if Task.isCancelled { return }
            guard let data, !data.isEmpty else {
                self.client?.urlProtocol(self, didFailWithError: URLError(.resourceUnavailable)); return
            }
            // Content is immutable (content-addressed), so it's freely cacheable.
            let resp = URLResponse(url: url, mimeType: nil, expectedContentLength: data.count, textEncodingName: nil)
            self.client?.urlProtocol(self, didReceive: resp, cacheStoragePolicy: .allowed)
            self.client?.urlProtocol(self, didLoad: data)
            self.client?.urlProtocolDidFinishLoading(self)
        }
    }

    override func stopLoading() { task?.cancel(); task = nil }
}

/// A dedicated session wired to the custom protocol (so loading is reliable — unlike
/// URLSession.shared + URLProtocol.registerClass, which iOS doesn't honor for custom
/// schemes). Memory-cached: immutable CIDs never need a refetch.
enum ContentSession {
    static let shared: URLSession = {
        let cfg = URLSessionConfiguration.ephemeral
        cfg.protocolClasses = [ContentURLProtocol.self]
        cfg.urlCache = URLCache(memoryCapacity: 48 << 20, diskCapacity: 0)
        cfg.requestCachePolicy = .returnCacheDataElseLoad
        return URLSession(configuration: cfg)
    }()

    static func url(_ cid: String) -> URL? { URL(string: "\(ContentURLProtocol.scheme)://\(cid)") }
}

/// Drop-in image view backed by the in-process content provider.
/// `ContentImage(cid: post.media[i]) { placeholder }`.
struct ContentImage<Placeholder: View>: View {
    let cid: String
    @ViewBuilder var placeholder: () -> Placeholder
    @State private var image: UIImage?

    var body: some View {
        Group {
            if let image {
                Image(uiImage: image).resizable()
            } else {
                placeholder()
            }
        }
        .task(id: cid) { await load() }
    }

    private func load() async {
        guard !cid.isEmpty, image == nil, let url = ContentSession.url(cid) else { return }
        guard let (data, _) = try? await ContentSession.shared.data(from: url),
              let img = UIImage(data: data) else { return }
        await MainActor.run { self.image = img }
    }
}
