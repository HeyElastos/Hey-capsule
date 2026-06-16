import SwiftUI
import UIKit
import AVFoundation

// Full-screen in-app voice-call UI — a 1:1 port of CallOverlay (MainActivity.kt:1001-1196).
//
// The call STATE is owned by App/CallController.swift (CallKit). CallOverlay is a
// pure presentation layer: it takes a `CallUIState` value + the action closures and
// renders the matching screen. The enum mirrors Android CallManager.State
// (CallManager.kt:29-42): idle / outgoing / incoming / active / groupActive.
//
// Engine touchpoints (1:1 with Android VoiceAudio + HeyApi.voicePeers):
//   • onAccept / onDecline / onEnd / onMute — wired by the owner (RootView).
//     The owner's handlers call engine.voiceStart / voiceStop / voiceSetMuted.
//   • The mic-link probe polls engine.voicePeers() every second (Android line 1104).

/// Mirror of Android CallManager.State (CallManager.kt:29-42). `.idle` renders nothing.
enum CallUIState: Equatable {
    case idle
    case outgoing(peer: String, name: String, callId: String)
    case incoming(peer: String, name: String, callId: String)
    /// `since` = the reference Date the call became active (drives the timer).
    case active(peer: String, name: String, callId: String, since: Date, isCaller: Bool)
    case groupActive(gid: String, callId: String, title: String, participants: [CallParticipant], since: Date)
}

/// A participant in a live group call (CallManager.GroupParticipant, CallManager.kt:27).
struct CallParticipant: Equatable, Hashable {
    var did: String
    var name: String
    var ticket: String = ""
    var mine: Bool = false
}

struct CallOverlay: View {
    let state: CallUIState
    var onMute: (Bool) -> Void = { _ in }          // new muted value
    var onEnd: () -> Void = {}                       // cancel / hang up / leave
    var onAccept: () -> Void = {}
    var onDecline: () -> Void = {}

    @EnvironmentObject private var store: AppStore

    private var displayName: String {
        switch state {
        case .outgoing(_, let name, _):           return name
        case .incoming(_, let name, _):           return name
        case .active(_, let name, _, _, _):       return name
        case .groupActive(_, _, let title, _, _): return title
        case .idle:                               return ""
        }
    }

    private var isGroup: Bool {
        if case .groupActive = state { return true }
        return false
    }

    var body: some View {
        if case .idle = state {
            EmptyView()
        } else {
            content
        }
    }

    private var content: some View {
        ZStack {
            // Gradient bg #0A1426 → #13233F (MainActivity.kt:1012).
            LinearGradient(colors: [Hey.callGradStart, Hey.callGradEnd],
                           startPoint: .top, endPoint: .bottom)
                .ignoresSafeArea()

            VStack(spacing: 0) {
                Spacer()

                // Peer avatar — gold gradient circle, group icon or initial (lines 1018-1023).
                ZStack {
                    Circle().fill(Hey.avatarGradient)
                    if isGroup {
                        Image(systemName: "person.3.fill")
                            .font(.system(size: 46, weight: .semibold))
                            .foregroundStyle(Hey.navy)
                    } else {
                        Text(initial(of: displayName))
                            .font(.system(size: 46, weight: .bold))
                            .foregroundStyle(Hey.navy)
                    }
                }
                .frame(width: 110, height: 110)

                Spacer().frame(height: 20)

                // Name (line 1025).
                Text(displayName.isEmpty ? "Unknown" : displayName)
                    .font(.system(size: 26, weight: .semibold))
                    .foregroundStyle(.white)

                Spacer().frame(height: 8)

                statusLine

                Spacer()

                controls

                Spacer().frame(height: 24)
            }
            .padding(28)
        }
    }

    // MARK: - Status line (lines 1027-1052)

    @ViewBuilder private var statusLine: some View {
        switch state {
        case .outgoing:
            subtle("Calling…")
        case .incoming:
            subtle("Incoming voice call")
        case .active(_, _, _, let since, _):
            CallTimer(since: since)
        case .groupActive(_, _, _, let participants, let since):
            let others = participants.filter { !$0.mine }.count
            subtle(others == 0 ? "Waiting for others…" : "\(others) connected")
            Spacer().frame(height: 6)
            CallTimer(since: since)
            if !participants.isEmpty {
                Spacer().frame(height: 18)
                HStack(spacing: 10) {
                    ForEach(participants.prefix(6), id: \.did) { p in
                        VStack(spacing: 4) {
                            ZStack {
                                Circle().fill(Color(hex: 0xFFFFFF, alpha: 0x33 / 255.0))
                                Text(initial(of: p.name.isEmpty ? Profile.short(p.did) : p.name))
                                    .font(.system(size: 16, weight: .bold))
                                    .foregroundStyle(.white)
                            }
                            .frame(width: 46, height: 46)
                            Text(p.mine ? "You" : (p.name.isEmpty ? Profile.short(p.did) : p.name))
                                .font(.system(size: 10))
                                .foregroundStyle(Color(hex: 0x9FB2D0))
                                .lineLimit(1)
                        }
                    }
                }
            }
        case .idle:
            EmptyView()
        }
    }

    // MARK: - Controls (lines 1054-1169)

    @ViewBuilder private var controls: some View {
        switch state {
        case .incoming:
            HStack(spacing: 56) {
                CallButton(systemIcon: "phone.down.fill", bg: Hey.callReject, label: "Decline",
                           action: onDecline)
                CallButton(systemIcon: "phone.fill", bg: Hey.callAccept, label: "Accept",
                           action: onAccept)
            }
        case .outgoing:
            CallButton(systemIcon: "phone.down.fill", bg: Hey.callReject, label: "Cancel",
                       action: onEnd)
        case .active(_, _, let callId, _, _):
            ActiveControls(callId: callId, leaveLabel: "Hang up",
                           onMute: onMute, onEnd: onEnd)
        case .groupActive(_, let callId, _, _, _):
            ActiveControls(callId: callId, leaveLabel: "Leave",
                           onMute: onMute, onEnd: onEnd)
        case .idle:
            EmptyView()
        }
    }

    private func subtle(_ text: String) -> some View {
        Text(text).font(.system(size: 15)).foregroundStyle(Color(hex: 0x9FB2D0))
    }

    private func initial(of name: String) -> String {
        let c = name.trimmingCharacters(in: .whitespaces).first
        return c.map { String($0).uppercased() } ?? "?"
    }
}

// MARK: - Active / GroupActive shared controls (lines 1069-1167)

private struct ActiveControls: View {
    let callId: String
    let leaveLabel: String
    var onMute: (Bool) -> Void
    var onEnd: () -> Void

    @EnvironmentObject private var store: AppStore
    @State private var muted = false
    @State private var speaker = false
    @State private var micGranted = false
    @State private var audioPeers = 0
    @State private var linkAge = 0

    var body: some View {
        VStack(spacing: 0) {
            // Mute / End / Speaker (lines 1090-1098, 1154-1162).
            HStack(spacing: 28) {
                CallButton(systemIcon: muted ? "mic.slash.fill" : "mic.fill",
                           bg: Hey.callBtnIdle, label: muted ? "Unmute" : "Mute") {
                    muted.toggle()
                    onMute(muted)
                    Task { await store.engine.voiceSetMuted(muted) }
                }
                CallButton(systemIcon: "phone.down.fill", bg: Hey.callReject, label: leaveLabel,
                           action: onEnd)
                CallButton(systemIcon: speaker ? "speaker.wave.2.fill" : "speaker.wave.1.fill",
                           bg: Hey.callBtnIdle, label: "Speaker") {
                    speaker.toggle()
                    setSpeaker(speaker)
                }
            }

            // Live audio-link probe (lines 1099-1116) — distinguishes mic from transport.
            if audioPeers == 0 {
                Spacer().frame(height: 10)
                Text(linkAge < 10
                     ? "connecting audio…"
                     : "audio link not forming — make sure both phones run the latest Hey")
                    .font(.system(size: 11))
                    .foregroundStyle(Color(hex: 0xFFFFFF, alpha: 0x88 / 255.0))
                    .multilineTextAlignment(.center)
            }

            // Mic-permission nudge (lines 1117-1133 / 1163-1166).
            if !micGranted {
                Spacer().frame(height: 12)
                Text("Allow microphone so you can be heard — tap to open settings")
                    .font(.system(size: 11))
                    .foregroundStyle(Color(hex: 0xFFD27A, alpha: 0xAA / 255.0))
                    .multilineTextAlignment(.center)
                    .onTapGesture { openAppSettings() }
            }
        }
        // Own the probe for the lifetime of the Active screen (DisposableEffect, line 1085/1149).
        .task(id: callId) {
            refreshMicPermission()
            audioPeers = 0
            linkAge = 0
            while !Task.isCancelled {
                audioPeers = await store.engine.voicePeers()
                linkAge += 1
                try? await Task.sleep(nanoseconds: 1_000_000_000)
            }
        }
    }

    private func refreshMicPermission() {
        micGranted = AVAudioSession.sharedInstance().recordPermission == .granted
        if !micGranted {
            AVAudioSession.sharedInstance().requestRecordPermission { granted in
                DispatchQueue.main.async { micGranted = granted }
            }
        }
    }

    private func setSpeaker(_ on: Bool) {
        // iOS routes voice audio via the audio session; mirror Android VoiceAudio.setSpeaker.
        try? AVAudioSession.sharedInstance().overrideOutputAudioPort(on ? .speaker : .none)
    }

    private func openAppSettings() {
        guard let url = URL(string: UIApplication.openSettingsURLString) else { return }
        UIApplication.shared.open(url)
    }
}

// MARK: - Call timer (lines 1175-1183)

private struct CallTimer: View {
    let since: Date
    @State private var elapsed: Int = 0

    var body: some View {
        Text(format(elapsed))
            .font(.system(size: 16))
            .foregroundStyle(Color(hex: 0x9FB2D0))
            .task(id: since) {
                while !Task.isCancelled {
                    elapsed = max(0, Int(Date().timeIntervalSince(since)))
                    try? await Task.sleep(nanoseconds: 1_000_000_000)
                }
            }
    }

    private func format(_ secs: Int) -> String {
        String(format: "%d:%02d", secs / 60, secs % 60)
    }
}

// MARK: - Call button (lines 1185-1194)

private struct CallButton: View {
    let systemIcon: String
    let bg: Color
    let label: String
    let action: () -> Void

    var body: some View {
        VStack(spacing: 8) {
            Button(action: action) {
                ZStack {
                    Circle().fill(bg)
                    Image(systemName: systemIcon)
                        .font(.system(size: 28, weight: .semibold))
                        .foregroundStyle(.white)
                }
                .frame(width: 68, height: 68)
            }
            .buttonStyle(.plain)
            Text(label)
                .font(.system(size: 12))
                .foregroundStyle(Color(hex: 0xB9C6DD))
        }
    }
}
