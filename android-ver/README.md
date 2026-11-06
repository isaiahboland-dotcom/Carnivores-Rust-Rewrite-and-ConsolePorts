# c2rs for Android (M19, first build)

The PC game (`c2-pc play`) in an Android NativeActivity: same renderer (wgpu,
Vulkan or GLES), audio (kira over AAudio) and menus, in the phone layout.

**No game files are in the APK.** It plays a game you built from **your own
mobile APK** (Carnivores: Dinosaur Hunter or Carnivores: Ice Age), copied to the
phone. A PC Carnivores 2 install or a standalone game is refused as the base: the
phone's own menu art and touch buttons come from the mobile game.

Status: builds, packages and signs. **Not yet run on a phone or emulator.**

## Build

Needs the Android SDK (a platform and build-tools), the NDK, Python 3 and
`rustup target add aarch64-linux-android`. No Gradle or Java project.

```sh
cd android-ver
ANDROID_HOME=~/Android/Sdk ./build-apk.sh      # -> target/c2rs.apk (arm64-v8a)
```

The script picks the newest NDK, build-tools and platform under `ANDROID_HOME`
(`ANDROID_NDK_HOME` overrides the NDK) and signs with Android's usual debug key
(`~/.android/debug.keystore`, made on first use).

## Put a game on the phone

1. Build the base game from your APK on the PC, using the launcher (**Build from my
   APK…**) or `c2-pc mobile-base YOUR.apk --out game`. The folder has a `HUNTDAT/`
   and a `from-apk.txt`.
2. Install the app and start it once, so Android makes its folder.
3. Copy the files into the app's folder:

```sh
adb install -r target/c2rs.apk
adb push game    /sdcard/Android/data/io.github.c2rs/files/game
adb push addons  /sdcard/Android/data/io.github.c2rs/files/addons   # optional
```

| In `files/` | |
|---|---|
| `game/` | the game built from your mobile APK (required) |
| `addons/` | add-on folders, combined over the game in name order (optional; prefix numbers to set the order) |
| `c2rs/` | saves, settings and combined games, made by the app |
| `c2rs.log` | the log, rewritten each start; check it when the app closes at once |

## Limits on Android (for now)

- **Base game:** built from a mobile APK only. Modded games are that base plus
  add-ons. Standalone games are not offered.
- **At most 8 animals and 8 weapons** per hunt (`--max-picks 8`). More gets too
  hectic on a phone. A full list ignores further taps; unpicking always works.
- **Touch:** the first finger acts as the mouse (tap = click, drag = look). The
  mobile game's own hunt controls (move stick, fire, weapon buttons) are not
  done yet. Back acts as Escape.
- arm64 only; Android 8.0 (API 26) or later.

## Testing

- Host: `cargo test` (launch rules: needs an APK-built game, the 8-pick limit, add-ons
  without standalones).
- Phone, each base (Dinosaur Hunter, Ice Age): vanilla (`game/` only) and
  modded (`game/` plus `addons/`). Check the menus, hunt setup picks (the 9th
  animal or weapon is refused), a hunt, sound, pausing and resuming (Home, then
  back to the app), and that saves survive a restart.
