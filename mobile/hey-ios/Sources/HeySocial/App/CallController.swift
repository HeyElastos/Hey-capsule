import Foundation
import CallKit
import AVFoundation

// CallKit bridge. A PushKit `voip` push MUST report an incoming call here or iOS
// terminates the app (docs/HEY_IOS_PUSH_GATEWAY.md §6). The caller identity is
// decrypted on-device from the sealed call-offer envelope (sealed-sender), not
// supplied by the gateway. Media rides the existing iroh carrier after answer.
final class CallController: NSObject {
    static let shared = CallController()
    private let provider: CXProvider
    private let callController = CXCallController()

    override init() {
        let cfg = CXProviderConfiguration()
        cfg.supportsVideo = false
        cfg.maximumCallsPerCallGroup = 1
        cfg.supportedHandleTypes = [.generic]
        provider = CXProvider(configuration: cfg)
        super.init()
        provider.setDelegate(self, queue: nil)
    }

    /// Called from PushKit. MUST report synchronously before `completion`.
    func reportIncomingCall(payload: [AnyHashable: Any], completion: @escaping () -> Void) {
        let uuid = UUID()
        // The real caller name comes from decrypting payload["e"] (sealed envelope);
        // placeholder until the decrypt path is wired.
        let display = (payload["from_hint"] as? String) ?? "Hey call"
        let update = CXCallUpdate()
        update.remoteHandle = CXHandle(type: .generic, value: display)
        update.hasVideo = false
        provider.reportNewIncomingCall(with: uuid, update: update) { error in
            if let error { print("reportNewIncomingCall failed: \(error)") }
            completion()
        }
    }
}

extension CallController: CXProviderDelegate {
    func providerDidReset(_ provider: CXProvider) {}

    func provider(_ provider: CXProvider, perform action: CXAnswerCallAction) {
        // Join the iroh voice topic from the call-offer envelope, then fulfill.
        action.fulfill()
    }

    func provider(_ provider: CXProvider, perform action: CXEndCallAction) {
        // Send "bye" on the call lane, tear down the iroh voice stream.
        action.fulfill()
    }
}
