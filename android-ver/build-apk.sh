#!/usr/bin/env bash
# Builds, packages and signs the Android app: target/c2rs.apk (arm64).
# Needs the Android SDK (build-tools, a platform) and the NDK, and
# `rustup target add aarch64-linux-android`. No game data goes in the APK.
#
#   ANDROID_HOME=~/Android/Sdk ./build-apk.sh [--debug]
set -euo pipefail
cd "$(dirname "$0")"

SDK="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Android/Sdk}}"
NDK="${ANDROID_NDK_HOME:-$(ls -d "$SDK"/ndk/* 2>/dev/null | sort -V | tail -1)}"
BT="$(ls -d "$SDK"/build-tools/* | sort -V | tail -1)"
PLATFORM="$(ls -d "$SDK"/platforms/android-* | sort -V | tail -1)"
API=26 # minSdkVersion in AndroidManifest.xml
PROFILE=release
[[ "${1:-}" == "--debug" ]] && PROFILE=debug
for need in "$NDK/toolchains/llvm" "$BT/aapt2" "$PLATFORM/android.jar"; do
    [[ -e "$need" ]] || { echo "missing: $need" >&2; exit 1; }
done
echo "SDK $SDK | NDK $(basename "$NDK") | build-tools $(basename "$BT") | $(basename "$PLATFORM")"

TC="$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$TC/aarch64-linux-android$API-clang"
export CC_aarch64_linux_android="$TC/aarch64-linux-android$API-clang"
export CXX_aarch64_linux_android="$TC/aarch64-linux-android$API-clang++"
export AR_aarch64_linux_android="$TC/llvm-ar"
FLAGS=(--target aarch64-linux-android -p c2-android)
[[ "$PROFILE" == release ]] && FLAGS+=(--release)
cargo build "${FLAGS[@]}"

OUT=target/apk
rm -rf -- "${OUT:?}"
mkdir -p "$OUT/lib/arm64-v8a"
cp "target/aarch64-linux-android/$PROFILE/libc2_android.so" "$OUT/lib/arm64-v8a/"
"$TC/llvm-strip" --strip-debug "$OUT/lib/arm64-v8a/libc2_android.so"

"$BT/aapt2" link -o "$OUT/unsigned.apk" --manifest AndroidManifest.xml -I "$PLATFORM/android.jar"
# Native libraries stored (not deflated) and 16 KiB aligned, so Android maps
# them straight from the APK (extractNativeLibs="false").
python3 -I - "$OUT/unsigned.apk" "$OUT" <<'PY'
import os, sys, zipfile
apk, root = sys.argv[1], sys.argv[2]
with zipfile.ZipFile(apk, "a") as z:
    for d, _, files in os.walk(os.path.join(root, "lib")):
        for f in files:
            p = os.path.join(d, f)
            z.write(p, os.path.relpath(p, root), compress_type=zipfile.ZIP_STORED)
PY
"$BT/zipalign" -f -P 16 4 "$OUT/unsigned.apk" "$OUT/aligned.apk"

# Android's usual debug key; made on first use.
KEY="$HOME/.android/debug.keystore"
if [[ ! -f "$KEY" ]]; then
    mkdir -p "$(dirname "$KEY")"
    keytool -genkeypair -keystore "$KEY" -storepass android -keypass android \
        -alias androiddebugkey -dname "CN=Android Debug,O=Android,C=US" \
        -keyalg RSA -keysize 2048 -validity 10000
fi
"$BT/apksigner" sign --ks "$KEY" --ks-pass pass:android --key-pass pass:android \
    --out target/c2rs.apk "$OUT/aligned.apk"
"$BT/apksigner" verify target/c2rs.apk
ls -l target/c2rs.apk
