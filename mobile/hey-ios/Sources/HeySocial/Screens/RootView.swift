import SwiftUI

// The app shell + the boot/lock state machine (1:1 with Android HeyApp, MainActivity.kt:240-740):
//   starting → locked (Secure Enclave unseal) → welcome (create/restore) → generating → ready.
// In .ready it's a 5-tab floating-dock shell (Chat·Feed·Verse·Wallet·You) with a top-bar
// activity bell, per-tab navigation, and the global verse-invite + call overlays.

struct RootView: View {
    @EnvironmentObject private var store: AppStore

    var body: some View {
        ZStack {
            switch store.phase {
            case .starting:
                SplashView()
            case .locked:
                LockView(onUnlock: { seed in Task { await store.unlock(seed: seed) } },
                         onRestore: { store.failUnlockToRestore() })
            case .welcome:
                OnboardingView(onCreate: { Task { await store.createIdentity() } },
                               onRestore: { phrase in Task { await store.restoreIdentity(phrase: phrase) } })
            case .generating:
                DidGenerationView(buttonLabel: "Enter Hey", onDone: { store.finishOnboarding() })
            case .ready:
                MainView()
            }
        }
        .animation(.easeInOut(duration: 0.3), value: store.phase)
    }
}

// MARK: - splash

private struct SplashView: View {
    @Environment(\.colorScheme) private var scheme
    var body: some View {
        ZStack {
            FrostBackground()
            VStack(spacing: 16) {
                Text("Hey").font(HeyFont.display).foregroundStyle(Hey.goldInk(scheme))
                ProgressView().tint(Hey.goldInk(scheme))
                Text("Starting your on-device runtime…")
                    .font(HeyFont.callout).foregroundStyle(Hey.muted(scheme))
            }
        }
    }
}

// MARK: - main shell

private struct MainView: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var scheme

    @State private var tab: HeyTab = .chat
    @State private var showActivity = false
    @State private var verseSheet: VerseSheetKind?
    @State private var openChatDid: String?       // cross-tab: open a chat from a profile
    @State private var callState: CallUIState = .idle

    var body: some View {
        ZStack(alignment: .bottom) {
            FrostBackground().ignoresSafeArea()

            VStack(spacing: 0) {
                if tab != .verse {
                    HeyTopBar(title: title, badge: store.activityBadge) {
                        store.activitySeen = store.activityCount
                        showActivity = true
                    }
                }
                content
            }

            FloatingDock(selected: $tab, unread: store.unread, online: store.online, onVerse: handleVerse)
                .padding(.bottom, 4)
        }
        // Verse dock-morph sheets.
        .sheet(item: $verseSheet) { kind in
            VerseSheetContainer(kind: kind, onClose: { verseSheet = nil }).environmentObject(store)
        }
        // Activity (the top-bar bell → notifications popup).
        .sheet(isPresented: $showActivity) {
            ActivitySheet(onOpenProfile: { did in showActivity = false; openProfileFromActivity(did) })
                .environmentObject(store)
        }
        // App-wide verse invite (pops up anywhere, like an incoming call).
        .overlay(alignment: .top) {
            if let invite = store.verseInvite {
                VerseInvitePopup(invite: invite,
                                 onAccept: { VerseLane.shared.accept(); store.verseInvite = nil; tab = .verse },
                                 onDecline: { VerseLane.shared.decline(); store.verseInvite = nil })
                    .padding(.top, 8).padding(.horizontal, 16)
                    .transition(.move(edge: .top).combined(with: .opacity))
            }
        }
        .animation(.spring(response: 0.4, dampingFraction: 0.85), value: store.verseInvite)
        // 1:1 voice call overlay (incoming from the call lane, or outgoing from a chat).
        .fullScreenCover(isPresented: Binding(get: { callState != .idle }, set: { if !$0 { callState = .idle } })) {
            CallOverlay(state: callState,
                        onMute: { muted in Task { await store.engine.voiceSetMuted(muted) } },
                        onEnd: { endCall() },
                        onAccept: { acceptCall() },
                        onDecline: { endCall() })
                .environmentObject(store)
        }
        .onChange(of: store.incomingCall) { signal in
            if let s = signal, s.type == "offer", callState == .idle {
                callState = .incoming(peer: s.from, name: Profile.short(s.from), callId: s.callId)
            } else if signal == nil, case .incoming = callState {
                callState = .idle
            }
        }
    }

    private var title: String {
        switch tab {
        case .chat:   return "Chat"
        case .feed:   return "Social"
        case .verse:  return "Verse"
        case .wallet: return "Wallet"
        case .you:    return "You"
        }
    }

    @ViewBuilder private var content: some View {
        switch tab {
        case .chat:
            ChatHost(openChatDid: $openChatDid, onCall: startCall)
        case .feed:
            FeedHost(onMessage: openChatFromProfile)
        case .verse:
            VerseView().ignoresSafeArea(edges: .top)
        case .wallet:
            WalletHost()
        case .you:
            ProfileHost(onMessage: openChatFromProfile)
        }
    }

    // MARK: cross-tab + verse + call coordination

    private func openChatFromProfile(_ did: String) {
        openChatDid = did
        tab = .chat
    }
    private func openProfileFromActivity(_ did: String) {
        // Route through the feed tab's profile stack.
        tab = .feed
        // (FeedHost handles a profile push on next appearance; minimal v1: surface the feed.)
    }

    private func handleVerse(_ action: VerseDockAction) {
        switch action {
        case .worlds:  verseSheet = .worlds
        case .invite:  verseSheet = .invite
        case .library: verseSheet = .library
        case .avatar:  VerseLane.shared.postUi("avatar")
        case .exit:    tab = .chat
        }
    }

    private func startCall(_ chat: Chat) {
        callState = .outgoing(peer: chat.id, name: chat.name, callId: "")
        Task {
            let ticket = await store.engine.peerTicket(did: chat.id)
            if !ticket.isEmpty { await store.engine.voiceStart(peerTicket: ticket, isCaller: true) }
        }
    }
    private func acceptCall() {
        guard case let .incoming(peer, name, callId) = callState else { return }
        callState = .active(peer: peer, name: name, callId: callId, since: Date(), isCaller: false)
        Task {
            let ticket = await store.engine.peerTicket(did: peer)
            if !ticket.isEmpty { await store.engine.voiceStart(peerTicket: ticket, isCaller: false) }
        }
    }
    private func endCall() {
        callState = .idle
        store.incomingCall = nil
        Task { await store.engine.voiceStop() }
    }
}

// MARK: - top bar

private struct HeyTopBar: View {
    @Environment(\.colorScheme) private var scheme
    let title: String
    let badge: Int
    let onBell: () -> Void

    var body: some View {
        HStack(spacing: 0) {
            Text(title).font(HeyFont.subtitle).foregroundStyle(Hey.ink(scheme))
            Spacer(minLength: 0)
            Button(action: onBell) {
                ZStack(alignment: .topTrailing) {
                    Image(systemName: "bell.fill").font(.system(size: 20)).foregroundStyle(Hey.muted(scheme))
                    if badge > 0 {
                        Text(badge > 99 ? "99+" : "\(badge)")
                            .font(.system(size: 9, weight: .bold)).foregroundStyle(.white)
                            .padding(.horizontal, 4).padding(.vertical, 1)
                            .background(Hey.like, in: Capsule())
                            .offset(x: 8, y: -6)
                    }
                }
            }
            .buttonStyle(.plain)
        }
        .padding(.horizontal, 20).padding(.top, 8).padding(.bottom, 6)
    }
}

// MARK: - per-tab navigation hosts

private struct ChatHost: View {
    @EnvironmentObject private var store: AppStore
    @Binding var openChatDid: String?
    let onCall: (Chat) -> Void

    @State private var path: [ChatRoute] = []
    @State private var addContact = false
    @State private var newGroup = false
    @State private var infoChat: Chat?

    enum ChatRoute: Hashable { case detail(Chat); case profile(String) }

    var body: some View {
        NavigationStack(path: $path) {
            ChatListView(onOpen: { path.append(.detail($0)) },
                         onAddContact: { addContact = true },
                         onNewGroup: { newGroup = true },
                         openVerse: {})
                .navigationDestination(for: ChatRoute.self) { route in
                    switch route {
                    case .detail(let chat):
                        ChatDetailView(chat: chat,
                                       onBack: { if !path.isEmpty { path.removeLast() } },
                                       onOpenInfo: { infoChat = chat },
                                       onCall: { onCall(chat) },
                                       onOpenProfile: { path.append(.profile($0)) })
                            .navigationBarBackButtonHidden(true)
                            .toolbar(.hidden, for: .navigationBar)
                    case .profile(let did):
                        UserProfileScreen(did: did,
                                          onBack: { if !path.isEmpty { path.removeLast() } },
                                          onMessage: { _ in })
                            .navigationBarBackButtonHidden(true)
                            .toolbar(.hidden, for: .navigationBar)
                    }
                }
                .toolbar(.hidden, for: .navigationBar)
        }
        .sheet(isPresented: $addContact) {
            AddContactSheet(onClose: { addContact = false },
                            onStartChat: { did in
                                addContact = false
                                path.append(.detail(Chat(id: did, name: Profile.short(did))))
                            })
                .environmentObject(store)
        }
        .sheet(isPresented: $newGroup) {
            NewGroupSheet(onClose: { newGroup = false }, onCreated: { newGroup = false })
                .environmentObject(store)
        }
        .sheet(item: $infoChat) { chat in
            ChatInfoHost(chat: chat,
                         onViewProfile: { infoChat = nil; if !chat.isGroup { path.append(.profile(chat.id)) } },
                         onCall: { infoChat = nil; onCall(chat) },
                         onDelete: { infoChat = nil; if !path.isEmpty { path.removeLast() } },
                         onClose: { infoChat = nil })
                .environmentObject(store)
        }
        .onChange(of: openChatDid) { did in
            guard let did else { return }
            path.append(.detail(Chat(id: did, name: Profile.short(did))))
            openChatDid = nil
        }
    }
}

private struct FeedHost: View {
    let onMessage: (String) -> Void
    @State private var path: [String] = []   // pushed profile dids

    var body: some View {
        NavigationStack(path: $path) {
            FeedView(onOpenProfile: { path.append($0) })
                .navigationDestination(for: String.self) { did in
                    UserProfileScreen(did: did,
                                      onBack: { if !path.isEmpty { path.removeLast() } },
                                      onMessage: { onMessage($0) })
                        .navigationBarBackButtonHidden(true)
                        .toolbar(.hidden, for: .navigationBar)
                }
                .toolbar(.hidden, for: .navigationBar)
        }
    }
}

private struct WalletHost: View {
    @EnvironmentObject private var store: AppStore
    @State private var evmSend: EvmSendTarget?
    @State private var elaSend = false
    @State private var beamSend = false

    var body: some View {
        WalletView(onSendEvm: { chain, token in evmSend = EvmSendTarget(chain: chain, token: token) },
                   onSendEla: { elaSend = true },
                   onSendBeam: { beamSend = true })
            .sheet(item: $evmSend) { t in
                SendSheet(chain: t.chain.key,
                          symbol: t.token?.symbol ?? t.chain.symbol,
                          network: t.chain.title,
                          token: t.token,
                          onClose: { evmSend = nil },
                          onSent: { evmSend = nil })
                    .environmentObject(store)
            }
            .sheet(isPresented: $elaSend) {
                ElaSendSheet(onClose: { elaSend = false }, onSent: { elaSend = false })
                    .environmentObject(store)
            }
            .sheet(isPresented: $beamSend) {
                BeamSendSheet(onClose: { beamSend = false }, onSent: { beamSend = false })
                    .environmentObject(store)
            }
    }
}

private struct EvmSendTarget: Identifiable {
    let chain: WalletChain
    let token: TokenBal?
    var id: String { chain.key + ":" + (token?.id ?? "native") }
}

private struct ProfileHost: View {
    @EnvironmentObject private var store: AppStore
    let onMessage: (String) -> Void

    @State private var path: [String] = []
    @State private var showSettings = false
    @State private var showEdit = false
    @State private var showQr = false

    var body: some View {
        NavigationStack(path: $path) {
            ProfileView(online: store.online, peers: store.peers,
                        onOpenSettings: { showSettings = true },
                        onOpenProfile: { path.append($0) },
                        onEdit: { showEdit = true },
                        onShowQr: { showQr = true })
                .navigationDestination(for: String.self) { did in
                    UserProfileScreen(did: did,
                                      onBack: { if !path.isEmpty { path.removeLast() } },
                                      onMessage: { onMessage($0) })
                        .navigationBarBackButtonHidden(true)
                        .toolbar(.hidden, for: .navigationBar)
                }
                .toolbar(.hidden, for: .navigationBar)
        }
        .sheet(isPresented: $showSettings) {
            SettingsHost(onClose: { showSettings = false }).environmentObject(store)
        }
        .sheet(isPresented: $showEdit) {
            if let me = store.me {
                EditProfileSheet(current: me, onSaved: { showEdit = false; Task { await store.refreshMe() } })
                    .environmentObject(store)
            }
        }
        .sheet(isPresented: $showQr) {
            if let me = store.me { MyQrSheet(did: me.did).environmentObject(store) }
        }
    }
}

// MARK: - sheet wrappers (nest sub-sheets so they stack correctly)

/// Wraps SettingsSheet so its QR / Connection / About entries stack as child sheets.
private struct SettingsHost: View {
    @EnvironmentObject private var store: AppStore
    let onClose: () -> Void
    @State private var showQr = false
    @State private var showConnection = false
    @State private var showAbout = false

    var body: some View {
        SettingsSheet(did: store.me?.did ?? "",
                      onClose: onClose,
                      onShowQr: { showQr = true },
                      onShowConnection: { showConnection = true },
                      onShowAbout: { showAbout = true })
            .sheet(isPresented: $showQr) { if let me = store.me { MyQrSheet(did: me.did).environmentObject(store) } }
            .sheet(isPresented: $showConnection) { ConnectionSheet(onClose: { showConnection = false }).environmentObject(store) }
            .sheet(isPresented: $showAbout) { AboutSheet(onClose: { showAbout = false }).environmentObject(store) }
    }
}

/// Wraps ChatInfoSheet so its Tip action stacks the TipSheet on top.
private struct ChatInfoHost: View {
    @EnvironmentObject private var store: AppStore
    let chat: Chat
    let onViewProfile: () -> Void
    let onCall: () -> Void
    let onDelete: () -> Void
    let onClose: () -> Void
    @State private var showTip = false

    var body: some View {
        ChatInfoSheet(chat: chat,
                      onViewProfile: onViewProfile,
                      onCall: onCall,
                      onTip: { showTip = true },
                      onDelete: onDelete,
                      onClose: onClose)
            .sheet(isPresented: $showTip) {
                TipSheet(authorDid: chat.id, authorName: chat.name, onClose: { showTip = false })
                    .environmentObject(store)
            }
    }
}

/// The top-bar bell → notifications, with profile pushes inside its own stack.
private struct ActivitySheet: View {
    let onOpenProfile: (String) -> Void
    @State private var path: [String] = []

    var body: some View {
        NavigationStack(path: $path) {
            NotificationsScreen(onOpenProfile: { path.append($0) }, onOpenPost: { _ in })
                .navigationDestination(for: String.self) { did in
                    UserProfileScreen(did: did,
                                      onBack: { if !path.isEmpty { path.removeLast() } },
                                      onMessage: { onOpenProfile($0) })
                        .navigationBarBackButtonHidden(true)
                        .toolbar(.hidden, for: .navigationBar)
                }
                .toolbar(.hidden, for: .navigationBar)
        }
    }
}

// MARK: - verse dock-morph sheets

private enum VerseSheetKind: Int, Identifiable { case worlds, invite, library; var id: Int { rawValue } }

private struct VerseSheetContainer: View {
    let kind: VerseSheetKind
    let onClose: () -> Void
    var body: some View {
        switch kind {
        case .worlds:  VerseWorldsSheet(onClose: onClose, onEnterWorld: { _ in onClose() })
        case .invite:  VerseInviteSheet(onClose: onClose)
        case .library: VerseLibrarySheet(onClose: onClose)
        }
    }
}

// MARK: - verse invite popup (app-wide)

private struct VerseInvitePopup: View {
    @Environment(\.colorScheme) private var scheme
    let invite: VerseInvite
    let onAccept: () -> Void
    let onDecline: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            Avatar(name: invite.name, size: 40)
            VStack(alignment: .leading, spacing: 2) {
                Text(invite.name).font(HeyFont.author).foregroundStyle(Hey.ink(scheme))
                Text("invites you to the \(invite.world)").font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))
            }
            Spacer()
            Button("Join", action: onAccept)
                .font(HeyFont.label).foregroundStyle(Hey.navy)
                .padding(.vertical, 8).padding(.horizontal, 14)
                .background(Hey.gold, in: Capsule())
            Button(action: onDecline) { Image(systemName: "xmark").font(.system(size: 13, weight: .bold)) }
                .foregroundStyle(Hey.muted(scheme))
        }
        .padding(12)
        .glass(HeyRadius.sheet)
    }
}
