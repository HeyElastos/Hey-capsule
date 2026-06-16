#!/usr/bin/env bash
# Build BEAM wallet-core (client library) from a PINNED tag + Hey's own thin JNI shim, for
# arm64-v8a + x86_64, and stage libbeam.so into the app's jniLibs. Run AFTER ./build-deps.sh.
#
# We build BEAM core from source (no prebuilt blob touches funds) and write our OWN shim (jni/).
# Exact BEAM CMake target/lib names vary by tag — the spots to confirm on the first local compile
# are marked  # ADJUST .
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
for _envf in "$HOME/Android/env.sh" "/var/home/linux/Android/env.sh"; do
  [ -f "$_envf" ] && { source "$_envf"; break; }
done
[ -d "${ANDROID_NDK_HOME:-}" ] || ANDROID_NDK_HOME="${NDK_HOME:-}"
[ -d "${ANDROID_NDK_HOME:-}" ] || { echo "ERROR: ANDROID_NDK_HOME ('${ANDROID_NDK_HOME:-}') is not an NDK dir. Run: unset ANDROID_NDK_HOME; then re-run (env.sh sets NDK_HOME), or export ANDROID_NDK_HOME=/real/ndk/path"; exit 1; }
export ANDROID_NDK_HOME

BEAM_TAG="${BEAM_TAG:-}"        # REQUIRED: pin a released tag, e.g. BEAM_TAG=beam-7.5.13882  (never HEAD)
ANDROID_API="${ANDROID_API:-24}"
# Cap parallel compilers — BEAM's C++ files use 1-2GB each. Set BEAM_JOBS=2 if RAM is tight.
BEAM_JOBS="${BEAM_JOBS:-4}"
ABIS=(arm64-v8a x86_64)
WORK="${BEAM_WORK:-$HERE/.work}"; DEPS="$HERE/deps"
BEAM_SRC="$WORK/beam"
TOOLCHAIN_FILE="$ANDROID_NDK_HOME/build/cmake/android.toolchain.cmake"
JNILIBS="$REPO/mobile/hey-social-app/app/src/main/jniLibs"
mkdir -p "$WORK"
# /tmp here is a small quota-capped tmpfs — keep compiler/cmake temps on the big disk.
export TMPDIR="$WORK/tmp"; mkdir -p "$TMPDIR"

[ -n "$BEAM_TAG" ] || { echo "ERROR: set BEAM_TAG to a pinned BEAM release tag (never HEAD)"; exit 1; }
[ -d "$DEPS/arm64-v8a/boost" ] || { echo "ERROR: run ./build-deps.sh first"; exit 1; }

# ── 1. fetch BEAM at the pinned tag (with submodules) ────────────────────────
if [ ! -d "$BEAM_SRC/.git" ]; then
  echo "==> clone BeamMW/beam @ $BEAM_TAG"
  git clone --branch "$BEAM_TAG" --depth 1 --recurse-submodules --shallow-submodules \
    https://github.com/BeamMW/beam "$BEAM_SRC"
fi

# ── 1b. namespace-clean (W6): bind the on-device node's listener to LOOPBACK only ──
# Hey runs a private BEAM node in-process; the wallet dials it over 127.0.0.1. NodeClient
# stock-binds INADDR_ANY (0.0.0.0:31744) — LAN-visible. Patch it to INADDR_LOOPBACK so the
# listener is 127.0.0.1-only: no externally-exposed socket. Idempotent (grep-guarded).
NODE_CLIENT_CPP="$BEAM_SRC/node/node_client.cpp"
if grep -q 'node.m_Cfg.m_Listen.ip(INADDR_ANY);' "$NODE_CLIENT_CPP"; then
  # BEAM io::Address::ip() takes HOST byte order: INADDR_LOOPBACK (0x7f000001) IS 127.0.0.1.
  # Do NOT htonl() it — on little-endian that becomes 1.0.0.127 and the listen bind fails with
  # EC_EADDRNOTAVAIL, crash-looping the node (it never syncs). Plain INADDR_LOOPBACK is correct.
  sed -i 's/node.m_Cfg.m_Listen.ip(INADDR_ANY);/node.m_Cfg.m_Listen.ip(INADDR_LOOPBACK);/' "$NODE_CLIENT_CPP"
  echo "==> patched node listener -> 127.0.0.1 loopback (W6) in $NODE_CLIENT_CPP"
elif grep -q 'node.m_Cfg.m_Listen.ip(INADDR_LOOPBACK);' "$NODE_CLIENT_CPP"; then
  echo "==> node listener already loopback-bound (W6) — skipping"
else
  echo "ERROR (W6): could not find 'node.m_Cfg.m_Listen.ip(INADDR_ANY);' in $NODE_CLIENT_CPP"
  echo "            the on-device node would bind 0.0.0.0 (LAN-visible). Refusing to build."
  exit 1
fi

# ── 1c. absolute sync % (W7): report height/target, not per-session relative ──
# OnSyncProgress() calls s.ToRelative(m_Done0), rebasing progress to the height the node
# was at when THIS session started. On a persistent (resuming) node that makes the bar
# snap to ~0% on every restart and climb to 100% over only the session's remaining blocks
# — hiding the true overall % (e.g. shows 100%→2% while really ~63% of the chain). Keep an
# absolute copy (sAbs) and report THAT to onSyncProgressUpdated; leave ToRelative + the
# onStartedNode gate untouched. Idempotent (grep-guarded).
if grep -q 'Node::SyncStatus sAbs = s;' "$NODE_CLIENT_CPP"; then
  echo "==> sync %% already absolute (W7) — skipping"
elif grep -q 'AdjustProgress(s.m_Done, s.m_Total);' "$NODE_CLIENT_CPP"; then
  # insert the absolute copy right after the SyncStatus fetch
  sed -i 's/                    Node::SyncStatus s = m_node.m_SyncStatus;/                    Node::SyncStatus s = m_node.m_SyncStatus;\n                    Node::SyncStatus sAbs = s; \/\/ Hey W7: absolute % for the bar/' "$NODE_CLIENT_CPP"
  # report the absolute copy instead of the relative one
  sed -i 's/                    AdjustProgress(s.m_Done, s.m_Total);/                    AdjustProgress(sAbs.m_Done, sAbs.m_Total);/' "$NODE_CLIENT_CPP"
  sed -i 's/m_model.m_observer->onSyncProgressUpdated(static_cast<int>(s.m_Done), static_cast<int>(s.m_Total));/m_model.m_observer->onSyncProgressUpdated(static_cast<int>(sAbs.m_Done), static_cast<int>(sAbs.m_Total));/' "$NODE_CLIENT_CPP"
  echo "==> patched sync %% -> absolute height\/target (W7) in $NODE_CLIENT_CPP"
else
  echo "WARN (W7): OnSyncProgress shape changed; sync %% left as BEAM default (relative)."
fi

# ── 2. reshape deps into BEAM's ANDROID layout ───────────────────────────────
# BEAM's CMake reads $ENV{BOOST_ROOT_ANDROID} / $ENV{OPENSSL_ROOT_DIR_ANDROID}, expects
#   <root>/include  +  <root>/libs/<abi>/...  and HARDCODES boost lib names
#   libboost_<comp>-clang-mt-<arch>-1_68.a (no override). We symlink our Boost 1.82 libs under
#   those 1_68 names (same Boost 1.82 throughout — just the filename BEAM insists on).
BEAM_BOOST="$WORK/beam-boost"; BEAM_OSSL="$WORK/beam-ossl"
rm -rf "$BEAM_BOOST" "$BEAM_OSSL"
mkdir -p "$BEAM_BOOST/include" "$BEAM_OSSL/include"
# includes are ABI-independent — link arm64-v8a's (normalize boost-<ver>/boost -> boost)
boost_inc_dir=$(find "$DEPS/arm64-v8a/boost/include" -maxdepth 1 -type d -name 'boost-*' | head -1)
[ -n "$boost_inc_dir" ] || boost_inc_dir="$DEPS/arm64-v8a/boost/include"
ln -sfn "$boost_inc_dir/boost" "$BEAM_BOOST/include/boost"
ln -sfn "$DEPS/arm64-v8a/openssl/include/openssl" "$BEAM_OSSL/include/openssl"
for abi in "${ABIS[@]}"; do
  case "$abi" in arm64-v8a) sfx=a64;; x86_64) sfx=x64;; x86) sfx=x32;; armeabi-v7a) sfx=a32;; *) sfx=a64;; esac
  mkdir -p "$BEAM_BOOST/libs/$abi" "$BEAM_OSSL/libs/$abi"
  for f in "$DEPS/$abi/boost/lib"/libboost_*.a; do
    [ -e "$f" ] || continue
    comp=$(basename "$f"); comp=${comp#libboost_}; comp=${comp%%[-.]*}   # libboost_system-...-1_82.a -> system
    ln -sfn "$f" "$BEAM_BOOST/libs/$abi/libboost_${comp}-clang-mt-${sfx}-1_68.a"
  done
  ln -sfn "$DEPS/$abi/openssl/lib/libcrypto.a" "$BEAM_OSSL/libs/$abi/libcrypto.a"
  ln -sfn "$DEPS/$abi/openssl/lib/libssl.a"    "$BEAM_OSSL/libs/$abi/libssl.a"
done
export BOOST_ROOT_ANDROID="$BEAM_BOOST"
export OPENSSL_ROOT_DIR_ANDROID="$BEAM_OSSL"
echo "==> BEAM deps wired: BOOST_ROOT_ANDROID=$BEAM_BOOST  OPENSSL_ROOT_DIR_ANDROID=$BEAM_OSSL"

# ── 3. build BEAM core (client lib) + our shim, per ABI ───────────────────────
for abi in "${ABIS[@]}"; do
  echo "==> [$abi] configure + build BEAM wallet-client"
  beam_build="$WORK/build-beam-$abi"
  cmake -G Ninja -S "$BEAM_SRC" -B "$beam_build" \
    -DCMAKE_TOOLCHAIN_FILE="$TOOLCHAIN_FILE" \
    -DANDROID_ABI="$abi" -DANDROID_PLATFORM="android-$ANDROID_API" -DANDROID_STL=c++_shared \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_EXPORT_COMPILE_COMMANDS=ON \
    -DBEAM_WALLET_CLIENT_LIBRARY=TRUE \
    -DBEAM_LELANTUS_SUPPORT=TRUE \
    -DBEAM_ATOMIC_SWAP_SUPPORT=FALSE \
    -DBEAM_ASSET_SWAP_SUPPORT=OFF \
    -DBEAM_LASER_SUPPORT=OFF \
    -DBEAM_IPFS_SUPPORT=OFF \
    -DBEAM_TESTS_ENABLED=FALSE \
    -DOPENSSL_USE_STATIC_LIBS=TRUE \
    -DOPENSSL_ROOT_DIR="$DEPS/$abi/openssl" \
    -DOPENSSL_INCLUDE_DIR="$DEPS/$abi/openssl/include" \
    -DOPENSSL_CRYPTO_LIBRARY="$DEPS/$abi/openssl/lib/libcrypto.a" \
    -DOPENSSL_SSL_LIBRARY="$DEPS/$abi/openssl/lib/libssl.a"
  # wallet_client doesn't pull mnemonic (decodeMnemonic) — build it too. Name `node` explicitly so
  # libnode.a (+ its deps) is always built for the on-device node. Fallback: build all.
  ninja -C "$beam_build" wallet_client mnemonic node || ninja -C "$beam_build"

  echo "==> [$abi] build Hey BEAM shim -> libbeam.so"
  shim_build="$WORK/build-shim-$abi"
  # Inherit BEAM's EXACT include dirs (incl. its bundled 3rdparty: secp256k1, etc.) from BEAM's own
  # compile commands, so the shim compiles against the same header set without chasing each include.
  beam_inc=$(grep -ho -- '-I[^ "]\+' "$beam_build/compile_commands.json" 2>/dev/null | sed 's/^-I//' | sort -u | tr '\n' ';')
  cmake -G Ninja -S "$HERE/jni" -B "$shim_build" \
    -DCMAKE_TOOLCHAIN_FILE="$TOOLCHAIN_FILE" \
    -DANDROID_ABI="$abi" -DANDROID_PLATFORM="android-$ANDROID_API" -DANDROID_STL=c++_shared \
    -DCMAKE_BUILD_TYPE=Release \
    -DBEAM_SRC="$BEAM_SRC" -DBEAM_BUILD="$beam_build" \
    -DBEAM_INCLUDES="$beam_inc" \
    -DBOOST_ANDROID="$BEAM_BOOST" -DOPENSSL_ANDROID="$BEAM_OSSL"
  ninja -C "$shim_build"

  echo "==> [$abi] stage .so"
  install -D "$shim_build/libbeam.so" "$JNILIBS/$abi/libbeam.so"
  # libc++_shared.so from the NDK (needed because we built with c++_shared).
  install -D "$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/$( \
    case "$abi" in arm64-v8a) echo aarch64-linux-android;; x86_64) echo x86_64-linux-android;; esac \
    )/libc++_shared.so" "$JNILIBS/$abi/libc++_shared.so"
done

echo "==> done. libbeam.so staged into $JNILIBS/{arm64-v8a,x86_64}."
echo "Now run the app build: cd $REPO/mobile/hey-social-app && ./build-apk.sh"
