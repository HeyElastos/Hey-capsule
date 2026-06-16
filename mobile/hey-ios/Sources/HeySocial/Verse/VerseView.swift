import SwiftUI

// Hosts the HeyVerse Godot world (mobile/hey-verse) inside SwiftUI.
//
// HeyVerse is a Godot 4.2 game. On iOS it embeds via Godot's iOS export +
// `HeyVerseGodotPlugin` (the Swift Godot plugin that mirrors HeyVersePlugin.kt) on
// the reused verse lane. Embedding the Godot engine as a library yields a
// UIViewController; we wrap it with UIViewControllerRepresentable.
//
// Until the Godot iOS framework + export templates are wired (Mac-side), this shows
// a placeholder so the rest of the app builds and runs. Replace `GodotPlaceholderVC`
// with the real Godot view controller from the export.
struct VerseView: View {
    @Environment(\.dismiss) private var dismiss
    var body: some View {
        ZStack(alignment: .topLeading) {
            GodotHost()
                .ignoresSafeArea()
            Button { dismiss() } label: {
                Image(systemName: "xmark").font(.system(size: 15, weight: .bold)).foregroundStyle(.white)
                    .padding(12).background(.black.opacity(0.35), in: Circle())
            }
            .padding(.top, 12).padding(.leading, 12)
        }
        .onAppear { /* Godot loop pulls VerseLane.shared.pollJSON() each frame */ }
    }
}

private struct GodotHost: UIViewControllerRepresentable {
    func makeUIViewController(context: Context) -> UIViewController {
        // TODO(mac): return the Godot iOS view controller from the embedded engine.
        // e.g. GodotViewController() configured to load res://home.tscn, with
        // HeyVerseGodotPlugin registered so GDScript can call the verse bridge.
        GodotPlaceholderVC()
    }
    func updateUIViewController(_ vc: UIViewController, context: Context) {}
}

/// Placeholder shown until the Godot engine is embedded (Mac build step).
private final class GodotPlaceholderVC: UIViewController {
    override func viewDidLoad() {
        super.viewDidLoad()
        let host = UIHostingController(rootView: Placeholder())
        addChild(host); host.view.frame = view.bounds
        host.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        view.addSubview(host.view); host.didMove(toParent: self)
    }
    private struct Placeholder: View {
        var body: some View {
            ZStack {
                LinearGradient(colors: [Hey.callGradStart, Hey.callGradEnd], startPoint: .top, endPoint: .bottom)
                VStack(spacing: 10) {
                    Image(systemName: "globe.americas.fill").font(.system(size: 48)).foregroundStyle(Hey.gold)
                    Text("HeyVerse").font(HeyFont.header).foregroundStyle(.white)
                    Text("Godot world embeds here (iOS export).")
                        .font(HeyFont.caption).foregroundStyle(.white.opacity(0.7))
                }
            }
        }
    }
}
