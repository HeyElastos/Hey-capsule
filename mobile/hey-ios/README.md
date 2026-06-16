# Hey Social — iOS app

Native SwiftUI port of the Android Hey Social app (`mobile/hey-social-app`). **Reuses the same Rust engine**
(`capsules/hey-mobile-runtime` + iroh carrier) byte-for-byte via a Swift FFI; only the UI + bridge are new.

> Design docs: [`../../docs/HEY_IOS_PORT_PLAN.md`](../../docs/HEY_IOS_PORT_PLAN.md),
> [`HEY_IOS_UI_PORT.md`](../../docs/HEY_IOS_UI_PORT.md), [`HEY_IOS_PUSH_GATEWAY.md`](../../docs/HEY_IOS_PUSH_GATEWAY.md),
> [`HEY_VERSE_IOS.md`](../../docs/HEY_VERSE_IOS.md).

## Status

**Skeleton (2026-06-09).** Authored on Linux; **compiles on macOS + Xcode only** (iOS SDK is Mac-only).
The UI builds and runs in the simulator **right now against `MockEngine`** — no Rust lib needed — so you can iterate on
look-and-feel before the engine FFI lands. Swap `MockEngine` → `RustEngine` once `HeyEngine.xcframework` is built.

## Layout

```
mobile/hey-ios/
  project.yml                       XcodeGen spec (app + Notification Service Extension targets)
  scripts/build-rust-xcframework.sh build the engine for device + simulator → HeyEngine.xcframework
  include/HeyEngine.h               C-ABI contract the Swift bridge imports (mirrors HeyApi.kt externs)
  Sources/
    HeySocial/                      the app target
      App/        HeyApp.swift, AppDelegate.swift (APNs + PushKit)
      Theme/      HeyTheme.swift    (verified tokens from HEY_IOS_UI_PORT.md)
      Bridge/     HeyEngine.swift (protocol + RustEngine), MockEngine.swift, HeyModels.swift
      Components/ FloatingDock.swift, GlassCard.swift, FrostBackground.swift, Avatar.swift
      Screens/    RootView, ChatListView, ChatDetailView, FeedView, WalletView, ProfileView
      Verse/      VerseView.swift (Godot host), HeyVersePlugin.swift (port of HeyVersePlugin.kt)
    HeyNotificationService/         the NSE target (wake → pull → decrypt single-shot)
  Resources/                        Info.plist, entitlements, Assets.xcassets (to add on Mac)
```

## Build (on a Mac)

```bash
# 0. tools
brew install xcodegen
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios

# 1. engine → xcframework (uses the +1.91 toolchain, iroh 1.0)
./scripts/build-rust-xcframework.sh           # produces Frameworks/HeyEngine.xcframework

# 2. generate the Xcode project from project.yml
xcodegen generate

# 3. open + run
open HeySocial.xcodeproj
```

To run the **UI only** before the engine is ready: in `project.yml` the app already links nothing native, and
`HeyEngine.live` returns `MockEngine()` until `RUST_ENGINE` is defined — just `xcodegen generate && open` and run.

## HeyVerse

HeyVerse (`../hey-verse`, Godot 4.2) embeds via Godot's iOS export + `Verse/HeyVersePlugin.swift` (a Swift Godot plugin
that mirrors `HeyVersePlugin.kt`) on the reused `hey_verse_send`/`hey_verse_poll` lane. Needs Godot iOS export templates.
See [`HEY_VERSE_IOS.md`](../../docs/HEY_VERSE_IOS.md). Path chosen: **Swift plugin (mirror Android)**.

## Engine FFI

`capsules/hey-mobile-runtime` currently exports JNI (`cfg(target_os="android")`). The iOS build adds a C-ABI surface
(`src/ios.rs`, skeleton committed) that mirrors those entry points; `build-rust-xcframework.sh` builds it. See
`include/HeyEngine.h` for the contract and the TODO list in `src/ios.rs`.
