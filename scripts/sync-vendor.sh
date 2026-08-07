#!/usr/bin/env bash
# Keep capsules/vendor/* byte-identical to hey-engine/vendor/*.
#
# WHY THIS EXISTS
# ---------------
# Cargo does not inherit [patch.crates-io] across workspaces, so every Hyper
# product repeats the engine's vendored transport fixes at its own workspace
# root (see the block in ../Cargo.toml). Hyper-Desktop and Hyper-Skia can point
# straight at ../hey-engine/vendor/*, because they are only ever built from a
# checkout that has hey-engine as a sibling.
#
# This pack cannot. It is fetched as a STANDALONE TARBALL by the YunoHost
# package (elastos-runtime_ynh fetch_hey_capsules), and peer-provider /
# blobs-provider are compiled ON THE VPS, where hey-engine does not exist. A
# `../hey-engine` path would silently resolve to nothing there and the box would
# build unpatched crates.io — which is the precise failure this whole mechanism
# exists to prevent. So the pack carries its own copies (~3 MB) and this script
# keeps them honest.
#
# Drift is not hypothetical: the pack sat on a vendored netdev 0.43.0 while the
# engine had moved to 0.45.0, so the patch was being silently ignored on a
# version mismatch.
#
# USAGE
#   ./scripts/sync-vendor.sh            # copy engine -> pack
#   ./scripts/sync-vendor.sh --check    # exit 1 if they differ (for CI)

set -euo pipefail

CRATES=(iroh iroh-gossip netdev noq-udp)

pack_root="$(cd "$(dirname "$0")/.." && pwd)"
engine_vendor="$(cd "$pack_root/../hey-engine/vendor" 2>/dev/null && pwd || true)"
pack_vendor="$pack_root/capsules/vendor"

if [ -z "$engine_vendor" ]; then
    echo "ERROR: hey-engine/vendor not found next to this pack." >&2
    echo "  expected: $pack_root/../hey-engine/vendor" >&2
    echo "  (this script is a DEV-TIME tool; the pack tarball is self-contained" >&2
    echo "   and does not need hey-engine to build)" >&2
    exit 2
fi

check_only=0
[ "${1:-}" = "--check" ] && check_only=1

# target/ is untracked build output (iroh-gossip's is multi-GB); .git never
# belongs in a vendored copy.
excludes=(--exclude=target --exclude=.git)

status=0
for crate in "${CRATES[@]}"; do
    src="$engine_vendor/$crate"
    dst="$pack_vendor/$crate"

    if [ ! -d "$src" ]; then
        echo "ERROR: missing engine vendor dir: $src" >&2
        exit 2
    fi

    if [ "$check_only" -eq 1 ]; then
        # -n dry-run + -i itemise: any output means the trees differ.
        # -c compares CONTENT; --no-times stops mtime alone from counting as a
        # difference. Both are required: git does not preserve mtimes, so on a
        # fresh CI checkout every file looks "modified" and a timestamp-sensitive
        # check would fail 100% of the time.
        drift="$(rsync -anic --no-times --delete "${excludes[@]}" "$src/" "$dst/" 2>/dev/null || true)"
        if [ -n "$drift" ]; then
            echo "DRIFT  $crate"
            echo "$drift" | sed 's/^/         /'
            status=1
        else
            echo "ok     $crate ($(grep -m1 '^version' "$dst/Cargo.toml" | cut -d'"' -f2))"
        fi
    else
        rsync -a --delete "${excludes[@]}" "$src/" "$dst/"
        echo "synced $crate ($(grep -m1 '^version' "$dst/Cargo.toml" | cut -d'"' -f2))"
    fi
done

if [ "$check_only" -eq 1 ] && [ "$status" -ne 0 ]; then
    echo >&2
    echo "capsules/vendor is out of sync with hey-engine/vendor." >&2
    echo "Run ./scripts/sync-vendor.sh, then bump the iroh/iroh-gossip versions in" >&2
    echo "capsules/{peer,blobs}-provider/Cargo.toml to match the vendored ones —" >&2
    echo "a version mismatch makes Cargo IGNORE the patch silently." >&2
fi

exit "$status"
