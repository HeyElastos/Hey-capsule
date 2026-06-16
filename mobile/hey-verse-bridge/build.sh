#!/usr/bin/env bash
# Build the bridge and install it into the Godot project.
# The .gdextension is only copied in once the .so exists, so a fresh checkout
# of the game never logs missing-library errors.
set -euo pipefail
cd "$(dirname "$0")"

cargo build --release

mkdir -p ../hey-verse/bin
cp target/release/libhey_verse_bridge.so ../hey-verse/bin/
cp hey_verse_bridge.gdextension ../hey-verse/

echo "installed -> ../hey-verse (restart Godot; net.gd will pick it up)"
echo "android:    cargo ndk -t arm64-v8a -o ../hey-verse/bin/android build --release"
