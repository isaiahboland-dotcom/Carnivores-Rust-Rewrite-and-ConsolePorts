#!/usr/bin/env bash
# Runs `c2-parity predation --count` on an Android device over adb: the
# headless predation bench at 20, 40 and 60 animals (docs/PREDATION-PERF.md).
# Copies the CLI and the original game's _RES.TXT, models and areas to
# /data/local/tmp/c2bench (nothing is installed; the game app is untouched).
#
#   android-ver/bench-predation.sh GAME_DIR [AREA_NUMBERS] [SECONDS] [COUNTS]
#   android-ver/bench-predation.sh source/c2-original "1 3 5" 300 20,40,60
#
# Needs the Android SDK (platform-tools) and the NDK; GAME_DIR holds HUNTDAT.
set -euo pipefail

GAME="${1:?usage: $0 GAME_DIR [AREAS] [SECONDS] [COUNTS]}"
AREAS="${2:-1 3 5}"
SECS="${3:-300}"
COUNTS="${4:-20,40,60}"
SDK="${ANDROID_HOME:-$HOME/Android/Sdk}"
NDK="${ANDROID_NDK_HOME:-$(ls -d "$SDK"/ndk/* 2>/dev/null | sort -V | tail -1)}"
ADB="$SDK/platform-tools/adb"
HERE="$(cd "$(dirname "$0")" && pwd)"
PC="$HERE/../pc-ver"
DEST=/data/local/tmp/c2bench

HUNTDAT="$(find "$GAME" -maxdepth 1 -iname huntdat -type d | head -1)"
[ -n "$HUNTDAT" ] || { echo "no HUNTDAT in $GAME" >&2; exit 1; }
"$ADB" get-state >/dev/null || { echo "no device on adb" >&2; exit 1; }

TC="$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin"
(cd "$PC" && CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$TC/aarch64-linux-android29-clang" \
    cargo build --release -p c2-parity --target aarch64-linux-android)

"$ADB" shell mkdir -p "$DEST/HUNTDAT/AREAS"
"$ADB" push "$PC/target/aarch64-linux-android/release/c2-parity" "$DEST/" >/dev/null
"$ADB" shell chmod 755 "$DEST/c2-parity"
for f in "$HUNTDAT"/_RES.TXT "$HUNTDAT"/*.CAR; do
    "$ADB" push "$f" "$DEST/HUNTDAT/" >/dev/null
done
for n in $AREAS; do
    for ext in MAP RSC; do
        "$ADB" push "$HUNTDAT/AREAS/AREA$n.$ext" "$DEST/HUNTDAT/AREAS/" >/dev/null
    done
done

"$ADB" shell getprop ro.product.model
for n in $AREAS; do
    echo "== AREA$n =="
    "$ADB" shell "$DEST/c2-parity" predation "$DEST/HUNTDAT/AREAS/AREA$n.MAP" "$SECS" \
        --count "$COUNTS" | grep -E "^(==|bench|state digest)"
done
