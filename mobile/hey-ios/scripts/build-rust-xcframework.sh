#!/usr/bin/env bash
# Build capsules/hey-mobile-runtime for iOS device + simulator and bundle as
# HeyEngine.xcframework. RUN ON macOS (needs the iOS SDK). Linux can author this
# but not run it.
#
# Prereqs:
#   rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
#   the +1.91 toolchain (iroh 1.0) — see capsules/hey-mobile-runtime/rust-toolchain.toml
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
IOS_DIR="$(dirname "$HERE")"
RT_DIR="$IOS_DIR/../../capsules/hey-mobile-runtime"   # the engine crate
OUT="$IOS_DIR/Frameworks"
LIB="libhey_mobile_runtime.a"            # cargo emits this for a staticlib crate-type
HDR="$IOS_DIR/include"                   # HeyEngine.h lives here

# NOTE: the engine crate currently builds [cdylib, rlib] for Android. For iOS add
# `staticlib` to its [lib] crate-type and wire `#[cfg(target_os="ios")] mod ios;`
# (see src/ios.rs). This script assumes that is done.

# Match the Android hardening (build-apk.sh): a Rust panic must NOT unwind across the
# extern "C" FFI boundary (UB) — abort instead; and overflow-checks on for release so an
# integer wrap on attacker-controlled wire data is a clean abort, not silent corruption.
# Frame pointers aid crash triage. (PAC/arm64e is dev/enterprise-only — not loadable for
# App Store builds — so we stay on aarch64-apple-ios.)
export RUSTFLAGS="${RUSTFLAGS:-} -C force-frame-pointers=yes"
HARDEN=(--config 'profile.release.panic="abort"' --config 'profile.release.overflow-checks=true')

echo "==> building device (aarch64-apple-ios)"
( cd "$RT_DIR" && cargo build --release "${HARDEN[@]}" --target aarch64-apple-ios )

echo "==> building simulator (aarch64-apple-ios-sim)"
( cd "$RT_DIR" && cargo build --release "${HARDEN[@]}" --target aarch64-apple-ios-sim )

DEV="$RT_DIR/target/aarch64-apple-ios/release/$LIB"
SIM="$RT_DIR/target/aarch64-apple-ios-sim/release/$LIB"

rm -rf "$OUT/HeyEngine.xcframework"
mkdir -p "$OUT"

echo "==> assembling HeyEngine.xcframework"
xcodebuild -create-xcframework \
  -library "$DEV" -headers "$HDR" \
  -library "$SIM" -headers "$HDR" \
  -output "$OUT/HeyEngine.xcframework"

echo "==> done: $OUT/HeyEngine.xcframework"
echo "    set RUST_ENGINE=1 (Swift active-compilation-conditions) to use RustEngine instead of MockEngine."
