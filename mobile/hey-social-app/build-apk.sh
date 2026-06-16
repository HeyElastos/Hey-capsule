#!/usr/bin/env bash
# Build the FULLY NATIVE Hey Social APK (Jetpack Compose, no WebView):
#   1. cross-compile the shared Rust runtime + app-API -> arm64 + x86_64 .so
#   2. stage both .so into jniLibs
#   3. gradle assembleRelease (signed with the keystore.properties release key,
#      R8-minified, non-debuggable). For a fast dev/debug build use:
#        ./gradlew --no-daemon assembleDebug
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
RT="$REPO/capsules/hey-mobile-runtime"

# env: tolerate running as root (no ~/Android/env.sh) + a stale ANDROID_NDK_HOME placeholder.
for _envf in "$HOME/Android/env.sh" "/var/home/linux/Android/env.sh"; do
  [ -f "$_envf" ] && { source "$_envf"; break; }
done
[ -d "${ANDROID_NDK_HOME:-}" ] || ANDROID_NDK_HOME="${NDK_HOME:-}"
: "${ANDROID_NDK_HOME:?set ANDROID_NDK_HOME or NDK_HOME (or provide ~/Android/env.sh)}"
export ANDROID_NDK_HOME
# Keep build temps OFF the small quota-capped /tmp tmpfs (rustc/clang + the gradle JVM).
export TMPDIR="${TMPDIR:-$REPO/.buildtmp}"; mkdir -p "$TMPDIR"
export GRADLE_OPTS="${GRADLE_OPTS:-} -Djava.io.tmpdir=$TMPDIR"

echo "==> 1. cross-compiling hey-mobile-runtime (arm64-v8a + x86_64, release, HARDENED)"
# ── GrapheneOS-grade native hardening (libhey_mobile_runtime.so only) ──────────
# force-frame-pointers : precise crash / MTE / GWP-ASan backtraces.
# -z relro,-z now      : full RELRO + eager binding (read-only GOT, no lazy PLT).
# -z noexecstack       : non-executable stack (W^X).
# panic="abort"        : NEVER unwind out of the cdylib across the JNI boundary
#                        (unwinding across extern "C" is UB) — fail closed on a
#                        bug instead of risking a corrupt-state continuation.
# overflow-checks      : integer overflow PANICS (-> abort) instead of silently
#                        wrapping into a buffer-length bug. Mobile-only via
#                        --config so the relay/desktop release profile is untouched.
export RUSTFLAGS="${RUSTFLAGS:-} -C force-frame-pointers=yes -C link-arg=-Wl,-z,relro -C link-arg=-Wl,-z,now -C link-arg=-Wl,-z,noexecstack"
( cd "$RT" && cargo ndk -t arm64-v8a -t x86_64 build --release \
    --config 'profile.release.panic="abort"' \
    --config 'profile.release.overflow-checks=true' \
    -p hey-mobile-runtime )

echo "==> 2. staging .so"
install -D "$REPO/target/aarch64-linux-android/release/libhey_mobile_runtime.so" \
    "$HERE/app/src/main/jniLibs/arm64-v8a/libhey_mobile_runtime.so"
install -D "$REPO/target/x86_64-linux-android/release/libhey_mobile_runtime.so" \
    "$HERE/app/src/main/jniLibs/x86_64/libhey_mobile_runtime.so"

if [ ! -f "$HERE/keystore.properties" ]; then
  echo "ERROR: keystore.properties missing — refusing to ship a debug-signed wallet." >&2
  echo "       Copy keystore.properties.example and point it at your release keystore." >&2
  exit 1
fi

echo "==> 3. gradle assembleRelease (signed, R8-minified, non-debuggable)"
( cd "$HERE" && ./gradlew --no-daemon assembleRelease )

APK="$HERE/app/build/outputs/apk/release/app-release.apk"
echo "==> done: $APK"; ls -lh "$APK"
echo "==> verify signer:"; "${JAVA_HOME:-}/bin/keytool" -printcert -jarfile "$APK" 2>/dev/null | grep -E "SHA256:|Owner:" || true
