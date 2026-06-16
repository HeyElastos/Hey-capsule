import SwiftUI

// Restore an existing account from its 12/24-word phrase — the iOS port of Android's
// RestoreScreen (MainActivity.kt:5627-5666). Re-derives did:key, did:elastos and the
// wallets on this device; nothing is uploaded. The phrase is validated by the engine
// (validateMnemonic) before we hand it back to the orchestrator via onRestore.
//
// Security mirrors Android: the field uses a no-autocorrect / no-suggestion keyboard so
// the recovery words never enter the IME's personalized dictionary, and the view is
// marked private to the recents/screenshot surface (.privacySensitive + redaction).
struct RestoreView: View {
    @EnvironmentObject private var store: AppStore
    @Environment(\.colorScheme) private var scheme

    let onBack: () -> Void
    let onRestore: (String) -> Void

    @State private var phrase = ""
    @State private var err = ""
    @State private var checking = false

    var body: some View {
        ZStack {
            FrostBackground()
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    Spacer().frame(height: 20)
                    HStack(spacing: 4) {
                        Button(action: onBack) {
                            Image(systemName: "arrow.left").font(.system(size: 18, weight: .semibold))
                                .foregroundStyle(Hey.ink(scheme)).frame(width: 40, height: 40)
                        }
                        Text("Restore your account").font(.system(size: 20, weight: .bold))
                            .foregroundStyle(Hey.ink(scheme))
                    }
                    Spacer().frame(height: 8)
                    Text("Enter your 12-word Hey recovery phrase. It re-derives your identity, your Elastos DID and your wallets on this device — nothing is uploaded.")
                        .font(HeyFont.callout).foregroundStyle(Hey.muted(scheme))
                        .lineSpacing(20 - 14)
                    Spacer().frame(height: 18)

                    // word1  word2  word3  …  (no autocorrect / no suggestions — keeps the
                    // recovery words out of the keyboard's learned dictionary)
                    ZStack(alignment: .topLeading) {
                        if phrase.isEmpty {
                            Text("word1  word2  word3  …")
                                .font(HeyFont.body).foregroundStyle(Hey.muted(scheme))
                                .padding(.horizontal, 14).padding(.vertical, 14)
                        }
                        TextEditor(text: $phrase)
                            .font(HeyFont.body).foregroundStyle(Hey.ink(scheme))
                            .scrollContentBackground(.hidden)
                            .autocorrectionDisabled(true)
                            .textInputAutocapitalization(.never)
                            .keyboardType(.asciiCapable)
                            .padding(.horizontal, 10).padding(.vertical, 6)
                            .onChange(of: phrase) { _ in err = "" }
                    }
                    .frame(height: 140)
                    .glass()
                    .privacySensitive()

                    if !err.isEmpty {
                        Spacer().frame(height: 10)
                        Text(err).font(HeyFont.caption).foregroundStyle(Hey.like)
                    }

                    Spacer().frame(height: 18)
                    Button(action: restore) {
                        Group {
                            if checking {
                                ProgressView().tint(Hey.navy)
                            } else {
                                Text("Restore").font(.system(size: 16, weight: .bold))
                            }
                        }
                        .foregroundStyle(Hey.navy)
                        .frame(maxWidth: .infinity).frame(height: 54)
                        .background(Hey.gold, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                    }
                    .disabled(checking)

                    Spacer().frame(height: 12)
                    Text("It's the same 12 words you can import into official Elastos Essentials.")
                        .font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))
                }
                .padding(.horizontal, 24)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .scrollContentBackground(.hidden)
        }
    }

    private func restore() {
        // normalize exactly like Android: trim, lowercase, collapse runs of whitespace.
        let p = phrase.trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
            .replacingOccurrences(of: "\\s+", with: " ", options: .regularExpression)
        checking = true
        Task {
            let valid = await store.engine.validateMnemonic(p)
            await MainActor.run {
                checking = false
                if !valid {
                    err = "That doesn't look like a valid 12-word recovery phrase. Check the words, spelling and order."
                } else {
                    onRestore(p)
                }
            }
        }
    }
}
