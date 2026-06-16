#!/usr/bin/env bash
# Build the native Hey Social APK end-to-end:
#   1. cross-compile the Rust mini-runtime -> arm64 .so
#   2. stage the .so + the prebuilt hey-social dist into the app
#   3. gradle assembleDebug
#
# Prereqs: ~/Android/env.sh (JAVA_HOME, ANDROID_HOME, NDK_HOME), rustup target
# aarch64-linux-android on the 1.91 toolchain, cargo-ndk.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"            # .../Hey
RT="$REPO/capsules/hey-mobile-runtime"
DIST="$REPO/capsules/hey-social/dist"

source ~/Android/env.sh
export ANDROID_NDK_HOME="${NDK_HOME:?set NDK_HOME via ~/Android/env.sh}"

echo "==> 1. cross-compiling hey-mobile-runtime (arm64-v8a + x86_64, release)"
( cd "$RT" && cargo ndk -t arm64-v8a -t x86_64 build --release -p hey-mobile-runtime )

echo "==> 2. staging .so + dist"
install -D "$REPO/target/aarch64-linux-android/release/libhey_mobile_runtime.so" \
    "$HERE/app/src/main/jniLibs/arm64-v8a/libhey_mobile_runtime.so"
install -D "$REPO/target/x86_64-linux-android/release/libhey_mobile_runtime.so" \
    "$HERE/app/src/main/jniLibs/x86_64/libhey_mobile_runtime.so"
rm -rf "$HERE/app/src/main/assets/dist" && mkdir -p "$HERE/app/src/main/assets/dist"
cp "$DIST"/* "$HERE/app/src/main/assets/dist/"

echo "==> 3. gradle assembleDebug"
( cd "$HERE" && ./gradlew --no-daemon assembleDebug )

APK="$HERE/app/build/outputs/apk/debug/app-debug.apk"
echo "==> done: $APK"
ls -lh "$APK"
