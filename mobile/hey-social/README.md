# Hey shell (Tauri v2 mobile)

**Architecture A — the "remote window".** This is a thin native app whose only job is
to load `hey-social` / `hey-chat` from your home **Elastos runtime** inside a WebView,
authenticated by a launch token. The capsule WASM runs *unmodified* against its serving
origin; Carrier P2P, content/IPFS, DID/wallet identity, and storage all stay on the
runtime. The phone holds **no keys and no P2P node** — it is a window, not a node.

Why this is nearly free: every backend call in the capsules goes to
`{window.location-origin}/api/provider/<scheme>/<op>` and auth is a launch token read
from `?home_token` that the runtime swaps for a first-party HttpOnly cookie. So "porting
to Android" is just: point a WebView at the right origin with the token. `useHttpsScheme`
keeps that cookie/localStorage jar from being wiped.

## Layout

```
mobile/hey-shell/
  src/                     launcher UI (vanilla HTML/JS/CSS, no build step)
    index.html  main.js  styles.css
  src-tauri/
    src/lib.rs             connect() → navigate webview to remote capsule; load_config()
    src/main.rs            entry → lib::run()
    tauri.conf.json        useHttpsScheme=true, withGlobalTauri=true, frontendDist=../src
    capabilities/default.json
    Cargo.toml             standalone crate ([workspace] detaches it from the pack)
```

## Prerequisites

Installed by `/tmp/hey-android-toolchain.sh` (sudo-free, under `$HOME`):
JDK 17 (Temurin), Android SDK (platform-34, build-tools 34, NDK 26, emulator),
Rust android targets, `cargo-ndk`, `tauri-cli` v2. Env lives in `~/Android/env.sh`.

```bash
source ~/Android/env.sh        # JAVA_HOME, ANDROID_HOME, NDK_HOME, PATH
```

## Build & run

```bash
cd mobile/hey-shell/src-tauri

# one-time: generate the Android Studio project
cargo tauri android init

# run on a connected device or a running emulator (live reload of the launcher)
cargo tauri android dev

# or produce an installable APK
cargo tauri android build --apk
# → src-tauri/gen/android/app/build/outputs/apk/universal/release/*.apk

# desktop build (handy for iterating on the launcher UI without a phone)
cargo tauri dev
```

## Using it

1. On the desktop, launch hey-social from Home and copy the URL — it contains
   `?home_token=<token>`. (Phase 1 will mint a single-use, audience-bound token and
   render it as a QR; see TODO.)
2. In the app: enter the **runtime host** (`https://your-node.nohost.me`), paste the
   **launch token**, pick **Social** or **Chat**, tap **Connect**.
3. The WebView navigates to the capsule and the session cookie is established. The
   inputs are remembered for next launch.

A deep link is also supported by the launcher prefill: opening the launcher with
`?host=…&token=…&app=hey-chat` populates the fields (wiring the `heyapp://` scheme to
the OS so a scanned QR opens the app directly is the next step — see TODO).

## Verification status (2026-05-30)

**Build: ✅ proven.** Toolchain (JDK 17, Android SDK 34, NDK 26.3, Rust android
targets, cargo-ndk, tauri-cli 2.11.2) installed sudo-free under `$HOME`. The Rust +
Gradle build compiles clean and produces an installable APK:

```
src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
package: os.elastos.hey.shell   minSdk 24   targetSdk 36   label "Hey"
launchable: os.elastos.hey.shell.MainActivity
uses-permission: android.permission.INTERNET   ← required for the remote window
native lib: lib/x86_64/libhey_shell_lib.so (frontend embedded via generate_context!)
```

**On-host run: ⚠️ blocked by the emulator, not the app.** The Android emulator's
bundled `qemu-system-x86_64` SIGSEGVs ~22 s into guest boot on this host (Fedora 44,
kernel 6.19). The coredump faults in the host GL/Vulkan render thread and the boot log
shows `cannot unmap ptr … protected range` — i.e. the SwiftShader/qemu JIT vs the
host's W^X memory enforcement. Tried and still crashing: `-gpu swiftshader_indirect`,
`-gpu off`, `ANDROID_EMULATOR_USE_SYSTEM_LIBS=1`, `-feature -Vulkan`,
`VK_LOADER_LAYERS_DISABLE=~all~` + llvmpipe. This is a host/emulator-binary issue,
independent of the app.

### Run it on a real device (works where the emulator can't)

`adb` is already installed. Plug in an Android phone with USB debugging on, then:

```bash
source ~/Android/env.sh
adb install -r \
  mobile/hey-shell/src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
adb shell am start -n os.elastos.hey.shell/.MainActivity
# live-reload dev against the device instead:
cd mobile/hey-shell/src-tauri && cargo tauri android dev
```

Alternative emulator paths if a device isn't handy: a Genymotion / Waydroid image, or
run the SDK emulator on a host with an older kernel. The APK itself is unchanged.

## TODO (Phase 1 → C)

- [ ] **QR launch**: register the `heyapp://connect?host=…&app=…&token=…` deep link
      (Android intent-filter via `tauri-plugin-deep-link`) so the system camera opens
      the app pre-paired. Optionally an in-app camera scanner (`plugin-barcode-scanner`).
- [ ] **Single-use token** binding on the runtime (`/session/start` consumes once,
      binds audience+nonce) so a screenshotted QR isn't replayable. *This is the one
      security change that lives in the runtime, not here.*
- [ ] **Back/switch**: Android back from a remote capsule returns to the launcher.
- [ ] **SPKI pinning** for the WebView connection (defeats network MITM on the cookie).
- [x] **Phase 2 (hybrid C) — self-hosted ntfy push (device side):** built +
      compile-verified. ntfy receiver ([src-tauri/src/push.rs](src-tauri/src/push.rs)),
      foreground service (`PushService.kt`) so it survives backgrounding,
      notifications, launcher ntfy fields, and local scripts
      ([scripts/ntfy-local.sh](scripts/ntfy-local.sh),
      [scripts/hey-push.sh](scripts/hey-push.sh)). Full design + the one remaining
      runtime hook: [docs/c-ntfy-push.md](../../docs/c-ntfy-push.md). Pending: on-device
      notification test (needs a phone) + the runtime POST-to-ntfy hook in `elastos-server`.
- [ ] **C remainder**: local read-cache for the feed; the runtime push hook; UnifiedPush
      as the battery-efficient successor to the always-on foreground service.
```
