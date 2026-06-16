#!/usr/bin/env bash
# Cross-compile BEAM's native deps (OpenSSL + Boost) STATIC for Android arm64-v8a + x86_64.
# This is the PRIMARY blocker for BEAM-on-Android (BEAM's own build_android.sh hardcodes a dev's
# prebuilt paths + a single ABI). Output layout consumed by build-beam.sh:
#   deps/<abi>/openssl/{include,lib}
#   deps/<abi>/boost/{include,lib}
#
# Prereqs: ANDROID_NDK_HOME, cmake, ninja, git, curl, perl, make. ~6GB + a long first run.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
# Match the app build's env convention (build-apk.sh sources ~/Android/env.sh -> NDK_HOME).
# Also check the project owner's home, since running as root makes $HOME=/root (no env.sh there).
for _envf in "$HOME/Android/env.sh" "/var/home/linux/Android/env.sh"; do
  [ -f "$_envf" ] && { source "$_envf"; break; }
done
# Prefer a VALID NDK directory: a stale ANDROID_NDK_HOME="..." placeholder must not win over NDK_HOME.
[ -d "${ANDROID_NDK_HOME:-}" ] || ANDROID_NDK_HOME="${NDK_HOME:-}"
[ -d "${ANDROID_NDK_HOME:-}" ] || { echo "ERROR: ANDROID_NDK_HOME ('${ANDROID_NDK_HOME:-}') is not an NDK dir. Run: unset ANDROID_NDK_HOME; then re-run (env.sh sets NDK_HOME), or export ANDROID_NDK_HOME=/real/ndk/path"; exit 1; }
export ANDROID_NDK_HOME

# OpenSSL 1.1.x is END-OF-LIFE (no security patches since 2023-09-11) — never ship
# it in a wallet .so. Default to the 3.0 LTS line (widest source compat with BEAM's
# older codebase; bump to a newer LTS once verified). If the pinned BEAM_TAG truly
# can't build against 3.x, move BEAM_TAG forward to a release that supports it
# rather than reintroducing 1.1.x. Override with OPENSSL_VERSION=... if needed.
OPENSSL_VERSION="${OPENSSL_VERSION:-3.0.15}"   # 3.0 LTS — replaces EOL 1.1.1w
# BEAM historically pinned Boost 1.68, but Boost.ContainerHash used std::unary_function
# (REMOVED in C++17) until it was guarded in Boost 1.81 — so anything <1.81 won't compile
# with a modern NDK (r26 libc++). 1.82 is the lowest the helper has ndk23 configs for AND
# that has the fix. BEAM's find_package(Boost 1.68) accepts a newer version.
BOOST_VERSION="${BOOST_VERSION:-1.82.0}"
ANDROID_API="${ANDROID_API:-24}"
ABIS=(arm64-v8a x86_64)
HOST_TAG="linux-x86_64"
TOOLCHAIN="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$HOST_TAG"
# Scratch can be GBs — set BEAM_WORK=/path/on/a/bigger/disk if your home is full.
WORK="${BEAM_WORK:-$HERE/.work}"; DEPS="$HERE/deps"
mkdir -p "$WORK" "$DEPS"
# Compilers write temp files to TMPDIR (defaults to /tmp). On this box /tmp is a small,
# quota-capped tmpfs — redirect temps onto the big disk so clang/ar don't hit EDQUOT.
export TMPDIR="$WORK/tmp"; mkdir -p "$TMPDIR"

# OpenSSL's Configure target + the clang arch triple per ABI.
ossl_target() { case "$1" in arm64-v8a) echo android-arm64;; x86_64) echo android-x86_64;; esac; }

# ── OpenSSL (static libssl + libcrypto) ──────────────────────────────────────
build_openssl() {
  local src="$WORK/openssl-$OPENSSL_VERSION"
  if [ ! -d "$src" ]; then
    local tgz="$WORK/openssl.tgz"
    # 1. Prefer a manually-downloaded tarball (this network 504s on the mirrors).
    #    Drop openssl-<ver>.tar.gz in your home, or set OPENSSL_TARBALL=/path.
    local found=""
    for c in "${OPENSSL_TARBALL:-}" \
             "$HOME/openssl-$OPENSSL_VERSION.tar.gz" "/root/openssl-$OPENSSL_VERSION.tar.gz" \
             "/var/home/linux/openssl-$OPENSSL_VERSION.tar.gz" \
             "$HERE/openssl-$OPENSSL_VERSION.tar.gz" "$PWD/openssl-$OPENSSL_VERSION.tar.gz"; do
      [ -n "$c" ] && [ -f "$c" ] && { found="$c"; break; }
    done
    if [ -n "$found" ]; then
      echo "==> using local OpenSSL tarball: $found"
      cp "$found" "$tgz"
    else
      echo "==> fetch OpenSSL $OPENSSL_VERSION"
      local tag="OpenSSL_${OPENSSL_VERSION//./_}"          # 1.1.1w -> OpenSSL_1_1_1w
      local urls=(
        "https://github.com/openssl/openssl/releases/download/$tag/openssl-$OPENSSL_VERSION.tar.gz"
        "https://www.openssl.org/source/old/${OPENSSL_VERSION%[a-z]}/openssl-$OPENSSL_VERSION.tar.gz"
        "https://www.openssl.org/source/openssl-$OPENSSL_VERSION.tar.gz"
      )
      local got=0
      for u in "${urls[@]}"; do
        echo "    trying $u"
        if curl -fSL --retry 3 --connect-timeout 20 "$u" -o "$tgz"; then got=1; break; fi
      done
      [ "$got" = 1 ] || { echo "ERROR: download failed. Put openssl-$OPENSSL_VERSION.tar.gz in \$HOME (or set OPENSSL_TARBALL=/path) and re-run."; exit 1; }
    fi
    tar -xzf "$tgz" -C "$WORK"
  fi
  for abi in "${ABIS[@]}"; do
    local out="$DEPS/$abi/openssl"
    [ -f "$out/lib/libcrypto.a" ] && { echo "==> OpenSSL $abi already built"; continue; }
    echo "==> build OpenSSL $abi"
    ( cd "$src" && make distclean >/dev/null 2>&1 || true
      export ANDROID_NDK_ROOT="$ANDROID_NDK_HOME"
      export PATH="$TOOLCHAIN/bin:$PATH"
      ./Configure "$(ossl_target "$abi")" -D__ANDROID_API__="$ANDROID_API" no-shared no-tests \
        --prefix="$out" --openssldir="$out"
      make -j"$(nproc)" build_libs
      make install_dev )
  done
}

# ── Boost 1.68 static (via the maintained Boost-for-Android helper) ───────────
# Building Boost for Android by hand (b2 + user-config.jam) is brittle; the helper handles the
# clang/NDK plumbing. It is a BUILD TOOL we run, not source we ship.
build_boost() {
  # moritz-wundke/Boost-for-Android: entrypoint `build-android.sh`, NDK as the FIRST
  # positional arg, downloads the Boost source itself, emits build/out/<arch>/{include,lib}.
  # (Its CLI matches --boost/--arch/--with-libraries; the dec1 fork does NOT — don't use it.)
  local helper="$WORK/boost4a"
  if [ ! -d "$helper" ]; then
    echo "==> clone Boost-for-Android helper (moritz-wundke)"
    git clone https://github.com/moritz-wundke/Boost-for-Android "$helper"
  fi
  # BEAM's Android CMake links exactly these Boost components (system/filesystem/program_options/
  # thread/regex/log/locale/date_time/coroutine); context is coroutine's dep, chrono/atomic back thread/log.
  local libs="system,filesystem,program_options,thread,regex,log,locale,date_time,coroutine,context,chrono,atomic"
  # If the network 504s on the Boost download too, drop a manually-downloaded
  # boost_<ver>.tar.bz2 in your home (or set BOOST_TARBALL=/path); the helper reuses it.
  local bver_u="${BOOST_VERSION//./_}"
  for c in "${BOOST_TARBALL:-}" "$HOME/boost_$bver_u.tar.bz2" "/root/boost_$bver_u.tar.bz2" \
           "/var/home/linux/boost_$bver_u.tar.bz2"; do
    [ -n "$c" ] && [ -f "$c" ] && { cp "$c" "$helper/boost_$bver_u.tar.bz2"; echo "==> using local Boost tarball: $c"; break; }
  done
  # NOTE: Boost 1.68 + a modern NDK is the FRAGILE part. If this fails, try a newer
  # --boost (BEAM tolerates >=1.68) or an older NDK. # VERIFY end-to-end before trusting.
  ( cd "$helper" && ./build-android.sh "$ANDROID_NDK_HOME" \
      --boost="$BOOST_VERSION" --arch=arm64-v8a,x86_64 --with-libraries="$libs" )
  for abi in "${ABIS[@]}"; do
    local out="$DEPS/$abi/boost"; mkdir -p "$out"
    cp -r "$helper/build/out/$abi/include" "$out/"   # build/out/<abi>/{include,lib}
    cp -r "$helper/build/out/$abi/lib" "$out/"
    echo "==> Boost $abi -> $out"
  done
}

build_openssl
build_boost
echo "==> deps done:"; ls -R "$DEPS" | head -40
echo "Next: ./build-beam.sh"
