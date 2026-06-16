import SwiftUI

// First-run welcome — the iOS port of Android's WelcomeFlow (MainActivity.kt:5528-5592).
// A swipeable 3-page intro ("Hey" / "Yours, end to end" / "Powered by ElastOS") with the
// create-new vs restore choice ALWAYS visible at the bottom. Shown BEFORE the runtime
// starts, so a restore can supply the seed first.
//
// Navigation is the orchestrator's: this view only fires `onCreate` (create a fresh
// identity) and `onRestore(phrase)` (restore from a recovery phrase). The restore screen
// is presented inline as a sheet the way the Android pager flips into RestoreScreen.
struct OnboardingView: View {
    @Environment(\.colorScheme) private var scheme
    let onCreate: () -> Void
    let onRestore: (String) -> Void

    @State private var page = 0
    @State private var restoreMode = false

    var body: some View {
        ZStack {
            FrostBackground()
            VStack(spacing: 0) {
                TabView(selection: $page) {
                    welcomePage.tag(0)
                    yoursPage.tag(1)
                    elastosPage.tag(2)
                }
                .tabViewStyle(.page(indexDisplayMode: .never))
                .frame(maxWidth: .infinity, maxHeight: .infinity)

                // page dots (MainActivity.kt:5575-5580)
                HStack(spacing: 8) {
                    ForEach(0..<3, id: \.self) { i in
                        Circle()
                            .fill(page == i ? Hey.goldInk(scheme) : Hey.muted(scheme).opacity(0.4))
                            .frame(width: page == i ? 9 : 7, height: page == i ? 9 : 7)
                    }
                }
                .padding(.vertical, 12)

                Button(action: onCreate) {
                    Text("Create new identity")
                        .font(.system(size: 16, weight: .bold)).foregroundStyle(Hey.navy)
                        .frame(maxWidth: .infinity).frame(height: 54)
                        .background(Hey.gold, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                }

                Spacer().frame(height: 10)

                Button { restoreMode = true } label: {
                    HStack(spacing: 8) {
                        Image(systemName: "key.fill").font(.system(size: 18))
                        Text("I have a recovery phrase").font(HeyFont.body)
                    }
                    .foregroundStyle(Hey.ink(scheme))
                    .frame(maxWidth: .infinity).frame(height: 50)
                    .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous)
                        .strokeBorder(Hey.glassBorder(scheme), lineWidth: 1))
                }

                Spacer().frame(height: 22)
            }
            .padding(.horizontal, 24)
            .padding(.top, 16)
        }
        // The Android pager flips into RestoreScreen in place; on iOS we present it as a
        // full-cover so the back button returns to the intro exactly like onBack.
        .fullScreenCover(isPresented: $restoreMode) {
            RestoreView(onBack: { restoreMode = false }, onRestore: { restoreMode = false; onRestore($0) })
        }
    }

    // ── page 0: the warm "Hey" welcome ──
    private var welcomePage: some View {
        ScrollView {
            VStack(spacing: 0) {
                Spacer(minLength: 40)
                ZStack {
                    Circle()
                        .fill(RadialGradient(colors: [Hey.gold.opacity(0.40), .clear],
                                             center: .center, startRadius: 0, endRadius: 95))
                        .frame(width: 150, height: 150)
                    Text("👋").font(.system(size: 76))
                }
                Spacer().frame(height: 8)
                Text("Hey").font(HeyFont.display).foregroundStyle(Hey.goldInk(scheme))
                Spacer().frame(height: 6)
                Text("a warm little corner of the internet that's truly yours 💛")
                    .font(HeyFont.bodyCopy).foregroundStyle(Hey.ink(scheme))
                    .multilineTextAlignment(.center).lineSpacing(HeyLineSpacing.bodyCopy)
                Spacer().frame(height: 12)
                Text("No ads, no snooping, no strangers in your data — just you and the people you love, safe on your own device. 🌿")
                    .font(HeyFont.callout).foregroundStyle(Hey.muted(scheme))
                    .multilineTextAlignment(.center).lineSpacing(HeyLineSpacing.callout)
                Spacer(minLength: 40)
            }
            .frame(maxWidth: .infinity)
        }
        .scrollContentBackground(.hidden)
    }

    // ── page 1: "Yours, end to end" — the three OnbRow guarantees ──
    private var yoursPage: some View {
        ScrollView {
            VStack(spacing: 0) {
                Spacer(minLength: 40)
                Text("Yours, end to end").font(HeyFont.subtitle.weight(.bold)).foregroundStyle(Hey.ink(scheme))
                Spacer().frame(height: 18)
                VStack(alignment: .leading, spacing: 10) {
                    OnbRow(icon: "key.fill",
                           title: "A self-sovereign identity",
                           body: "A did:key generated and held only on your phone.")
                    OnbRow(icon: "lock.fill",
                           title: "End-to-end encrypted",
                           body: "Post-quantum DMs + signed posts. No middleman can read them.")
                    OnbRow(icon: "icloud.slash.fill",
                           title: "No servers, no accounts",
                           body: "Your data lives with you and your friends — nowhere else.")
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(16).glass()
                Spacer(minLength: 40)
            }
            .frame(maxWidth: .infinity)
        }
        .scrollContentBackground(.hidden)
    }

    // ── page 2: "Powered by ElastOS" ──
    private var elastosPage: some View {
        ScrollView {
            VStack(spacing: 0) {
                Spacer(minLength: 40)
                HStack(spacing: 8) {
                    Image(systemName: "globe").font(.system(size: 26)).foregroundStyle(Hey.goldInk(scheme))
                    Text("Powered by ElastOS").font(.system(size: 18, weight: .bold)).foregroundStyle(Hey.ink(scheme))
                }
                Spacer().frame(height: 12)
                Text("ElastOS is a decentralized internet where you — not companies — own your identity, data, and money. Your phone is the node. One recovery phrase is your sovereign identity and wallet across the whole network.")
                    .font(HeyFont.callout).foregroundStyle(Hey.muted(scheme))
                    .multilineTextAlignment(.center).lineSpacing(HeyLineSpacing.callout)
                Spacer(minLength: 40)
            }
            .frame(maxWidth: .infinity)
        }
        .scrollContentBackground(.hidden)
    }
}

// Port of OnbRow (MainActivity.kt:5788-5798): gold icon + title + muted body.
private struct OnbRow: View {
    @Environment(\.colorScheme) private var scheme
    let icon: String
    let title: String
    let body: String

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: icon).font(.system(size: 22)).foregroundStyle(Hey.gold).frame(width: 22)
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.system(size: 15, weight: .semibold)).foregroundStyle(Hey.ink(scheme))
                Text(body).font(HeyFont.caption).foregroundStyle(Hey.muted(scheme))
                    .lineSpacing(18 - 13)
            }
        }
    }
}
