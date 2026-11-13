# c2rs for Android (M19, first build)

The PC game (`c2-pc play`) in an Android NativeActivity: same renderer (wgpu,
Vulkan or GLES), audio (kira over AAudio) and menus, in the phone layout.

**No game files are in the APK.** It plays a game you built from **your own
mobile APK** (Carnivores: Dinosaur Hunter or Carnivores: Ice Age), copied to the
phone. A PC Carnivores 2 install or a standalone game on its own is refused as the
base: the phone's own menu art and touch buttons come from the mobile game. A
standalone game can be combined beside that base on the PC (below).

Status: runs on a Retroid Pocket 5 (Android 13, Adreno 650, Vulkan): menus in the phone layout, and
hunter registration with the soft keyboard. Hunts are not yet checked on the device.

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

### From the launcher

Save an instance whose base game is built from your APK (add-ons, standalones and a
balance are all fine), then **Build** > **Android**. Put `c2rs.apk` (from
`build-apk.sh`) in the launcher's tools folder, or leave it in `android-ver/target/`
when running the launcher from this tree.

- **Build Android package** writes `<output>/<instance>/`: `c2rs.apk` and `files/`
  (`game/`, plus `balance.toml` if the instance has a balance). Install the APK,
  start it once, then copy the contents of `files/` to
  `Android/data/io.github.c2rs/files/`.
- **Name on the phone** (optional, per instance): empty, the package is the shared
  `io.github.c2rs` app and each install replaces the last. Named, the instance is an
  app of its own, `io.github.c2rs.<name>` (letters and digits, the rest as `_`), with
  its own `Android/data/<package>/files/`, so several instances sit side by side.
  The launcher relabels `c2rs.apk` with the SDK's newest build tools (`aapt2`,
  `zipalign`, `apksigner`, a platform's `android.jar`) and signs it with
  `~/.android/debug.keystore`, made with `keytool` if missing. It also links the
  game's launcher icon (`HUNTDAT/MOBILE/ICON.PNG`, from the player's own APK) into
  the app as `@mipmap/ic_launcher`; the engine APK from `build-apk.sh` has none.
  Without the SDK, an unnamed instance uses `c2rs.apk` as it is. Renaming an instance
  later makes another app; uninstall the old one by hand.
- **Build and install over adb** does that on the connected device (USB debugging
  on; adb from the tools folder, `PATH` or `$ANDROID_HOME/platform-tools`). The game
  is pushed as `game.new` and swapped in for `game/` only when complete. Saves
  (`c2rs/`), `addons/` and any other folders are left alone. Close the game first:
  installing the APK stops it.

### By hand

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
| `game/` | the game built from your mobile APK, or that game with standalones combined beside it (required) |
| `addons/` | add-on folders, combined over the game in name order (optional; prefix numbers to set the order) |
| `balance.toml` | prices and scores, as the launcher's balance editor writes them (optional) |
| `c2rs/` | saves, settings and combined games, made by the app |
| `c2rs.log` | the log, started fresh at each launch |

With no `game/` (or one not built from an APK), the app says so on screen; tap to close.

### A standalone game beside the mobile base

Standalones are combined on the PC, not on the phone (combining copies the whole
game, too slow at every start). Combine them beside the APK-built base, then copy
the result as `game/`:

```sh
c2-pc combine STANDALONE_DIR... --base game --out game-combined
adb push game-combined /sdcard/Android/data/io.github.c2rs/files/game
```

The combined game keeps the base's `from-apk.txt`, so the app takes it, and its
menus stay the phone's. The standalone's maps, animals and weapons come after the
base's. Where they have no hexagon picture of the mobile game's kind, the hexagon
shows their own menu picture (`AREAn`/`DINOn`/`WEAPONn.TGA`), cut to its shape. First checked with Carnivores Anachronous beside Dinosaur Hunter
(12 areas, 28 animals, 15 weapons).

### Full artwork cards

For standalone art with stats printed into the dinosaur pictures (such as Genesis
Redux 2.0), set `mobile_card_layout = true` under the recipe's `[settings]`.
Rebuild the recipe and use the updated Android APK. The map, dinosaur and weapon selectors show six
uncropped pictures per page, in two columns of three, with selection outlines,
page arrows, Back/Next and hunter points. Daylight and equipment options stay
available below the cards. The same option is in the launcher recipe editor’s
Advanced page, labeled “Mobile card layout”. Other recipes retain the honeycomb.
The built game's `HUNTDAT/_MOBILE_CARDS.TXT` carries this setting to Android.

### Weapon restrictions in recipes

Match weapons by their `_RES.TXT` names (case-insensitive):

```toml
[[settings.weapon_restrictions]]
name = "Shotgun"
allow_tranq = false
allow_trophy = false
```

Omitted flags preserve the weapon's existing behavior (normally both allowed).
`allow_tranq = false` keeps that weapon lethal even with Tranquilizer selected;
it also keeps its full lethal ammo supply in Half Tranq mode. Other carried
weapons still use their selected ammo. `allow_trophy = false` awards normal kill
points but creates no trophy and sends no collection ship. The finishing shot's
weapon decides, including projectiles that arrive after switching guns.
Unknown or ambiguous weapon names fail the recipe build instead of silently
ignoring a restriction. Rebuild both the recipe data and engine APK.

Genesis Redux's Compound Bow is named `Turok-Bow` in its resource table, and
its Over-Under Shotgun is `Shotgun`. Its configured points-only guns are
`Shotgun` and `Rex-Wrecker`.

## Limits on Android (for now)

- **Base game:** built from a mobile APK only. Modded games are that base plus
  add-ons, and standalones combined beside it on the PC (above).
- **At most 8 animals and 8 weapons** per hunt (`--max-picks 8`). More gets too
  hectic on a phone. A full list ignores further taps; unpicking always works.
- **Touch:** in the menus the first finger acts as the mouse. A hunt has the
  mobile game's own buttons in its places: the compass (map), binoculars under
  it (fire there while a weapon is in hand), the move stick (pushed all the way:
  run; drag anywhere else to look), the call top right and the weapon under it
  (tap: call / take out or put away; hold: the hexagon picker, slide and let
  go), and the "menu" tab (Restart, Resume, Escape!, all tappable). Options stay
  in the main menu. No jump or crouch buttons yet. Back acts as Escape; the soft keyboard opens for the
  hunter's name.
- **Controllers** (built-in like the Retroid's, or Bluetooth): the PC's pad
  layout (`c2-pad`). The on-screen buttons hide as soon as a pad, or a key, is
  used, and come back on the next touch.
- arm64 only; Android 8.0 (API 26) or later.

## Performance

The Video options' **Resolution** (Android only, 50-100%) draws fewer pixels and
lets the phone scale the picture up. On a Retroid Pocket 5 the GPU sets the pace;
see `docs/DECISIONS.md` (Android rendering performance) for what was measured.

- `c2rs.log` gets a line every 5 seconds: frames per second, the game's own time
  per frame and the time spent waiting on the GPU and the display.
- `files/args.txt` (one argument per line) replaces the launch arguments, for
  repeatable measurements, e.g. the map viewer at a fixed view:

  ```text
  area
  /storage/emulated/0/Android/data/io.github.c2rs/files/game/HUNTDAT/AREAS/AREA2.MAP
  --view
  384
  ```

  or a hunt that lands the same way each run (`hunt`, the map, `--seed`, `6`).
  Delete the file to play normally.
- The APK is profileable from adb: `simpleperf record --app io.github.c2rs`.

## Testing

- Host: `cargo test` (launch rules: needs an APK-built game, the 8-pick limit, add-ons
  without standalones).
- Phone, each base (Dinosaur Hunter, Ice Age): vanilla (`game/` only) and
  modded (`game/` plus `addons/`). Check the menus, hunt setup picks (the 9th
  animal or weapon is refused), a hunt, sound, pausing and resuming (Home, then
  back to the app), and that saves survive a restart.
