#!/usr/bin/env bash
# install-steamdeck.sh — Carnivores Rust rewrite: Steam Deck installer
#
# Repo: https://github.com/isaiahboland-dotcom/Carnivores-Rust-Rewrite-and-ConsolePorts
#
# What it does (every step is idempotent — safe to re-run, e.g. after a SteamOS update):
#   1. Installs Rust via rustup into $HOME (survives SteamOS updates) if missing
#   2. Installs build deps via pacman (SteamOS: disables the read-only root,
#      then re-enables it). SteamOS-specific breakage is handled automatically:
#      stale pacman locks, the stale-database state where pacman says
#      "up to date" while the files are gone (section 2a), and unowned
#      base-image files blocking installs ("exists in filesystem" — retried
#      with --overwrite limited to the exact conflicting paths). SteamOS
#      updates wipe these packages — just re-run the script.
#   3. Clones / updates the repo at ~/Games/c2rs/clone
#   4. Builds c2-launcher + c2-pc (cargo build --release -p c2-launcher -p c2-pc);
#      skipped when the binaries exist and sources haven't changed
#   5. Prepares ~/Games/c2rs/source/ (drop your Carnivores 2 / Ice Age files here)
#      and ~/Games/c2rs/mods/ (add-ons)
#   6. Writes the launch wrapper ~/Games/c2rs/c2rs-launch.sh — it execs the
#      compiled c2-launcher directly (no cargo, no PATH assumptions, fast start)
#   7. Adds a Desktop Mode application entry and offers to add it to Steam
#
# Run it in Desktop Mode (Konsole):
#   curl -fsSL https://raw.githubusercontent.com/isaiahboland-dotcom/Carnivores-Rust-Rewrite-and-ConsolePorts/main/steamdeck/installscript.sh | bash
# or download it first and run:  bash installscript.sh
#
# Update an existing install later (pull + rebuild only — the toolchain/deps
# checks above it stay silent when everything is already present):
#   curl -fsSL https://raw.githubusercontent.com/isaiahboland-dotcom/Carnivores-Rust-Rewrite-and-ConsolePorts/main/steamdeck/installscript.sh | bash -s -- --update
#
# Environment overrides:
#   INSTALL_ROOT=/some/path   install somewhere else (default ~/Games/c2rs — no spaces!)
#   FORCE_BUILD=1             rebuild even if binaries exist and sources unchanged
#
set -Eeuo pipefail

REPO_URL="https://github.com/isaiahboland-dotcom/Carnivores-Rust-Rewrite-and-ConsolePorts.git"
SCRIPT_URL="https://raw.githubusercontent.com/isaiahboland-dotcom/Carnivores-Rust-Rewrite-and-ConsolePorts/main/steamdeck/installscript.sh"
INSTALL_ROOT="${INSTALL_ROOT:-$HOME/Games/c2rs}"
CLONE_DIR="$INSTALL_ROOT/clone"
RELEASE_DIR="$CLONE_DIR/pc-ver/target/release"
GAME_DIR="$INSTALL_ROOT/source"
MODS_DIR="$INSTALL_ROOT/mods"
WRAPPER="$INSTALL_ROOT/c2rs-launch.sh"
DESKTOP_FILE="$HOME/.local/share/applications/c2rs.desktop"
FORCE_BUILD="${FORCE_BUILD:-0}"

LAUNCHER="$RELEASE_DIR/c2-launcher"   # GUI: mod config + play
GAME_BIN="$RELEASE_DIR/c2-pc"         # the game (spawned by the launcher)

# ---------- helpers ----------
ask() {  # ask <question> [default y|n] -> prints the answer
  local q="$1" d="${2:-n}" a=""
  if [ -t 0 ]; then
    read -r -p "$q [$d] " a || true
  elif [ -e /dev/tty ]; then      # curl | bash: stdin is the script, ask on the tty
    printf '%s [%s] ' "$q" "$d" >&2
    read -r a < /dev/tty || true
  fi                              # no tty at all: fall back to the default
  echo "${a:-$d}"
}
die() { printf '\nERROR: %s\n' "$*" >&2; exit 1; }
trap 'die "command failed at line $LINENO: $BASH_COMMAND"' ERR

# ---------- pacman helpers (lock + conflict auto-recovery; sections 2 / 2a) ----------
PACMAN_DB_LCK="/var/lib/pacman/db.lck"

# Remove a stale pacman database lock (left behind by an interrupted run) or
# wait for a live pacman to finish; otherwise every pacman call fails with
# "unable to lock database".
pacman_lock_ok() {
  [ -e "$PACMAN_DB_LCK" ] || return 0
  if command -v pgrep >/dev/null 2>&1 && pgrep -x pacman >/dev/null 2>&1; then
    echo "Another pacman is running; waiting for it to release the database lock ..."
    local i
    for i in 1 2 3 4 5 6 7 8 9 10; do
      sleep 6
      [ -e "$PACMAN_DB_LCK" ] || return 0
    done
    echo "ERROR: pacman is still holding $PACMAN_DB_LCK after 60s." >&2
    return 1
  fi
  echo "Removing stale pacman database lock ($PACMAN_DB_LCK) ..."
  sudo rm -f "$PACMAN_DB_LCK"
}

# pacman -Sy --needed with automatic recovery for SteamOS-specific failures.
# If untracked leftover files from the SteamOS base image block the install
# ("exists in filesystem" WITHOUT an owning package), retry once with
# --overwrite limited to exactly those paths. Conflicts with files owned by
# another installed package are never overwritten. Returns pacman's exit code.
pacman_install() {  # pacman_install <pkg> [<pkg> ...]
  local tmp rc p conflicts ow=()
  pacman_lock_ok || return 1
  tmp=$(mktemp)
  rc=0
  sudo pacman -Sy --needed --noconfirm "$@" >"$tmp" 2>&1 || rc=$?
  if [ "$rc" = 0 ]; then
    tr '\r' '\n' <"$tmp" | grep -vE '^[^A-Za-z]*$' || true
    rm -f "$tmp"; return 0
  fi
  # collect UNOWNED conflicting paths; lines naming an owner are skipped
  conflicts=$(sed -n '/exists in filesystem/{/owned by/b;/in local package/b;s/^[^:]*: \(\/[^ ]*\) exists in filesystem.*/\1/p;}' "$tmp" | sort -u)
  if [ -n "$conflicts" ]; then
    echo "Untracked SteamOS base-image files blocked the install; retrying with --overwrite for exactly those paths:" >&2
    printf '  %s\n' $conflicts >&2
    while IFS= read -r p; do ow+=(--overwrite "$p"); done <<< "$conflicts"
    rc=0
    sudo pacman -Sy --needed --noconfirm "${ow[@]}" "$@" >"$tmp" 2>&1 || rc=$?
    if [ "$rc" = 0 ]; then
      tr '\r' '\n' <"$tmp" | grep -vE '^[^A-Za-z]*$' || true
      rm -f "$tmp"; return 0
    fi
  fi
  tr '\r' '\n' <"$tmp" | grep -vE '^[^A-Za-z]*$' >&2 || true
  rm -f "$tmp"
  return "$rc"
}

# ---------- 0. preflight ----------
[ "$(uname -m)" = "x86_64" ] || die "This installer targets x86_64 (Steam Deck / PC)."
case "$INSTALL_ROOT" in /*) ;; *) INSTALL_ROOT="$PWD/$INSTALL_ROOT" ;; esac   # absolutize
case "$INSTALL_ROOT" in *[[:space:]]*) die "INSTALL_ROOT must not contain spaces: $INSTALL_ROOT" ;; esac

# A stray PKG_CONFIG export breaks every build script that honors it: PKG_CONFIG
# must name the pkg-config *binary* (search directories go in PKG_CONFIG_PATH).
# If it holds anything else — e.g. a colon-separated directory list — ignore it.
case "${PKG_CONFIG:-}" in
  "") ;;
  *:*)
    echo "NOTE: ignoring bogus PKG_CONFIG='$PKG_CONFIG' — it must name the pkg-config binary (search paths belong in PKG_CONFIG_PATH)."
    unset PKG_CONFIG ;;
  *)
    if [ ! -x "$PKG_CONFIG" ]; then
      echo "NOTE: ignoring non-executable PKG_CONFIG='$PKG_CONFIG'."
      unset PKG_CONFIG
    fi ;;
esac

if ! grep -qi 'steam' /etc/os-release 2>/dev/null; then
  [ "$(ask "This doesn't look like SteamOS. Continue anyway?" n)" = y ] || die "aborted"
fi

free_kb=$(df -Pk "$HOME" | awk 'NR==2 {print $4}')
if [ "${free_kb:-99999999}" -lt 3000000 ]; then
  [ "$(ask "Less than 3 GB free in $HOME — the Rust toolchain plus the release build can take several GB. Continue?" n)" = y ] || die "aborted"
fi

# ---------- 1. Rust toolchain (rustup, under $HOME — survives SteamOS updates) ----------
if ! command -v cargo >/dev/null; then
  if [ -f "$HOME/.cargo/env" ]; then
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"    # rustup installed earlier, just not on PATH here
  fi
fi
if ! command -v cargo >/dev/null; then
  echo "Rust not found."
  [ "$(ask 'Install Rust via rustup into $HOME?' y)" = y ] || die "Rust is required to build."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi
command -v cargo >/dev/null || die "cargo is still not on PATH — open a new terminal and re-run."

# ---------- 2. build deps (README: toolchain, pkg-config, alsa headers, libudev headers) ----------
need_pkgs=()
command -v git        >/dev/null || need_pkgs+=(git)
command -v curl       >/dev/null || need_pkgs+=(curl)
command -v pkg-config >/dev/null 2>&1 || command -v pkgconf >/dev/null 2>&1 || need_pkgs+=(pkgconf)   # Arch package name; ships the pkg-config alias
command -v cc >/dev/null || command -v gcc >/dev/null || need_pkgs+=(base-devel)
[ -f /usr/include/alsa/asoundlib.h ] || need_pkgs+=(alsa-lib)    # audio   (alsa-sys)
[ -f /usr/include/libudev.h ]      || need_pkgs+=(systemd-libs)  # gamepads (gilrs / libudev-sys)

if [ "${#need_pkgs[@]}" -gt 0 ]; then
  echo "Missing build tools: ${need_pkgs[*]}"
  if [ "$(ask 'Install them via pacman? (SteamOS: this disables the read-only root, then re-enables it.)' y)" = y ]; then
    if command -v steamos-readonly >/dev/null; then sudo steamos-readonly disable; fi
    if ! pacman_install "${need_pkgs[@]}"; then
      if command -v steamos-readonly >/dev/null; then sudo steamos-readonly enable || true; fi
      echo >&2 "pacman failed. If it complains about keys/signatures, try:"
      echo >&2 "    sudo pacman-key --init && sudo pacman-key --populate"
      die "could not install: ${need_pkgs[*]} — install them manually and re-run"
    fi
    if command -v steamos-readonly >/dev/null; then sudo steamos-readonly enable || true; fi
    if [ ! -f /usr/include/alsa/asoundlib.h ] || [ ! -f /usr/include/libudev.h ]; then
      echo "WARN: expected headers are still missing after pacman — attempting the SteamOS database repair below." >&2
    fi
  else
    die "missing build dependencies: ${need_pkgs[*]}"
  fi
fi

# ---------- 2a. verify the libs are REALLY usable (SteamOS stale-DB repair) ----------
# SteamOS updates reset /usr while pacman's database (in /var) survives — and a
# pacman install that ran with the read-only root still enabled leaves the same
# state. Result: pacman reports "up to date -- skipping" forever while alsa.pc,
# libudev.pc and the headers are gone, so every cargo build dies with
# "Package 'alsa' was not found in the pkg-config search path" no matter how
# often the packages are "reinstalled". Verify what the build scripts actually
# probe (pkg-config --exists alsa / libudev); on failure, drop ONLY the stale
# database entries (pacman -R --dbonly touches no files) and reinstall for real.
dep_libs_ok() {
  local pc=""
  command -v pkg-config >/dev/null 2>&1 && pc="pkg-config"
  if [ -z "$pc" ]; then command -v pkgconf >/dev/null 2>&1 && pc="pkgconf"; fi
  [ -n "$pc" ] || return 1
  "$pc" --exists alsa    >/dev/null 2>&1 || return 1
  "$pc" --exists libudev >/dev/null 2>&1 || return 1
  return 0
}

if ! dep_libs_ok; then
  repair_list=(alsa-lib systemd-libs)
  command -v pkg-config >/dev/null 2>&1 || command -v pkgconf >/dev/null 2>&1 || repair_list+=(pkgconf)
  echo "pkg-config can't find alsa and/or libudev even though pacman claims they're installed."
  echo "(A SteamOS update, or an install blocked by the read-only root, wiped the files"
  echo " while pacman's database kept saying 'installed' — so 'reinstalling' reports"
  echo " 'up to date -- skipping' forever.) Forcing a real reinstall of: ${repair_list[*]}"
  pacman_lock_ok || die "pacman database is locked by another process — wait a minute and re-run."
  if command -v steamos-readonly >/dev/null; then sudo steamos-readonly disable; fi
  for p in "${repair_list[@]}"; do
    sudo pacman -Rdd --dbonly --noconfirm "$p" >/dev/null 2>&1 || true   # stale DB entry only, no files touched
  done
  if ! pacman_install "${repair_list[@]}"; then
    if command -v steamos-readonly >/dev/null; then sudo steamos-readonly enable || true; fi
    die "reinstall of ${repair_list[*]} failed — run it manually to see why:
    sudo pacman -Sy --needed ${repair_list[*]}
If it reports 'exists in filesystem', retry with --overwrite '/usr/lib/lib*'."
  fi
  if command -v steamos-readonly >/dev/null; then sudo steamos-readonly enable || true; fi
  dep_libs_ok || die "alsa/libudev are STILL not findable. Run these and report the output:
    pkg-config --modversion alsa libudev
    ls -l /usr/lib/pkgconfig/alsa.pc /usr/lib/pkgconfig/libudev.pc
    pacman -Qkk alsa-lib systemd-libs"
  echo "Repaired — alsa and libudev are findable again."
fi

# The Rust 'pkg-config' crate execs the binary named by $PKG_CONFIG (default:
# 'pkg-config'). If only the pkgconf binary exists (its pkg-config alias is
# missing), point the crate at it for the builds below.
if ! command -v pkg-config >/dev/null 2>&1 && command -v pkgconf >/dev/null 2>&1; then
  export PKG_CONFIG="$(command -v pkgconf)"
fi

# ---------- 2b. --update: refresh an existing install ----------
# Same curl trust path as the installer itself — no separate updater script
# to keep in sync. Placed BEFORE the clone step on purpose: if nothing is
# installed yet, --update must not silently half-install (clone + build, but
# no wrapper/desktop entry). Toolchain/deps checks above run first: silent
# when present (the usual case), and they catch packages a SteamOS update
# wiped instead of letting the build fail with a cryptic alsa-sys error.
case "${1:-}" in
  --update)
    [ -d "$CLONE_DIR/.git" ] || die "Nothing installed at $CLONE_DIR yet — run the installer once without --update first."
    git -C "$CLONE_DIR" pull --ff-only || die "git pull failed — fix the local tree, or delete $CLONE_DIR and re-run the installer."
    if ! ( cd "$CLONE_DIR/pc-ver" && cargo build --release -p c2-launcher -p c2-pc ); then
      die "cargo build failed — if the errors mention alsa / libudev / pkg-config, run the full installer once (it detects and repairs the SteamOS package state), then retry --update."
    fi
    [ -x "$LAUNCHER" ] || die "build did not produce $LAUNCHER"
    echo "Updated. HEAD: $(git -C "$CLONE_DIR" rev-parse --short HEAD)"
    echo "Binary mtime: $(stat -c %y "$LAUNCHER")"
    exit 0
    ;;
esac

# ---------- 3. clone / update (idempotent) ----------
mkdir -p "$INSTALL_ROOT" "$GAME_DIR" "$MODS_DIR"

old_head=""
if [ ! -d "$CLONE_DIR/.git" ]; then
  if [ -e "$CLONE_DIR" ]; then
    die "$CLONE_DIR exists but isn't a git clone — move it aside or set INSTALL_ROOT."
  fi
  echo "Cloning (shallow) ..."
  git clone --depth 1 "$REPO_URL" "$CLONE_DIR"
else
  old_head=$(git -C "$CLONE_DIR" rev-parse HEAD)
  git -C "$CLONE_DIR" pull --ff-only || echo "WARN: couldn't fast-forward; keeping the local tree." >&2
fi
new_head=$(git -C "$CLONE_DIR" rev-parse HEAD)

[ -d "$CLONE_DIR/pc-ver" ] || die "unexpected repo layout: no pc-ver/ under $CLONE_DIR"

# ---------- 4. build the launcher AND c2-pc (the launcher spawns c2-pc) ----------
# One cargo invocation builds both, in sync. Console / VR crates are deliberately
# skipped (not needed on Deck). 'cargo run' is NOT used at launch time — see the
# wrapper below.
need_build=0
if [ "$FORCE_BUILD" = "1" ]; then
  need_build=1
elif [ ! -x "$LAUNCHER" ] || [ ! -x "$GAME_BIN" ]; then
  need_build=1
elif [ "$old_head" != "$new_head" ]; then   # the pull moved sources under us
  need_build=1
fi

if [ "$need_build" = 1 ]; then
  echo "Building launcher + game (release). On a Deck this takes a while — 15-30+ minutes is normal ..."
  if ! ( cd "$CLONE_DIR/pc-ver" && cargo build --release -p c2-launcher -p c2-pc ); then
    die "cargo build failed — see the errors above. If they mention 'alsa', 'libudev' or 'pkg-config', just re-run this script: it verifies the system libraries are genuinely installed and repairs the SteamOS stale-package state automatically."
  fi
else
  echo "Binaries present and sources unchanged — skipping build (set FORCE_BUILD=1 to force)."
fi
[ -x "$LAUNCHER" ] || die "build did not produce $LAUNCHER"
[ -x "$GAME_BIN" ]  || die "build did not produce $GAME_BIN"

# ---------- 5. game folder hint (don't fail if empty) ----------
if [ ! -d "$GAME_DIR/HUNTDAT" ]; then
  cat <<EOF

Heads-up: $GAME_DIR has no HUNTDAT/ folder yet.
Copy your own Carnivores 2 (or Carnivores: Ice Age) install into it — the
folder that contains HUNTDAT/ — then launch the app, make a New instance
and choose this folder on its Base game page:
    $GAME_DIR
Mods go in:
    $MODS_DIR/   (folders or .zip; extract .rar/.7z first)

EOF
fi

# ---------- 6. wrapper: launch the GUI, from the release dir ----------
# Why the compiled binary and not 'cargo run'? Steam shortcuts start with a
# minimal environment: ~/.cargo/bin may not be on PATH, cargo re-checks the
# build on every launch, and its output goes to a terminal nobody sees.
# c2-launcher resolves its sibling tools (c2-pc, c2-cook, ...) "beside itself,
# in the tools folder, or in this source tree's build folders" — so the pair
# must stay in target/release/ and the wrapper cds there first.
cat > "$WRAPPER" <<EOF
#!/usr/bin/env bash
set -e
cd "$RELEASE_DIR"
exec ./c2-launcher "\$@"

# In the launcher:
#   Settings        -> add $MODS_DIR as a mod and add-on folder
#   New instance    -> Base game page: choose $GAME_DIR (the folder containing HUNTDAT/)
#                      Add-ons page: add folders from $MODS_DIR, reorder; Save and exit
#   Instance box    -> Play
# The launcher remembers your add-on folder locally between launches; each
# instance keeps its own saves under ~/.local/share/c2rs/ (survives SteamOS
# updates).
EOF
chmod +x "$WRAPPER"

# ---------- 7. desktop entry (Desktop Mode launcher menu) ----------
mkdir -p "$(dirname "$DESKTOP_FILE")"
cat > "$DESKTOP_FILE" <<EOF
[Desktop Entry]
Type=Application
Name=Carnivores (Rust rewrite)
Comment=Fan-made Rust rewrite of Carnivores 2 — GUI launcher for mods + play
Exec=$WRAPPER
Path=$INSTALL_ROOT
Terminal=false
Categories=Game;
EOF
command -v update-desktop-database >/dev/null && update-desktop-database "$(dirname "$DESKTOP_FILE")" || true

# One-click path to Gaming Mode (SteamOS ships this helper; skip if absent).
if command -v steamos-add-to-steam >/dev/null; then
  if [ "$(ask 'Add it to Steam now (shows up as a non-Steam game)?' y)" = y ]; then
    steamos-add-to-steam "$DESKTOP_FILE" || echo "WARN: couldn't add to Steam — add it manually (see step 2 below)." >&2
  fi
fi

# ---------- 8. done ----------
cat <<EOF

Installed. Next steps:
  1. Copy your Carnivores 2 (or Ice Age) files into:
         $GAME_DIR          (must contain HUNTDAT/)
     and any mods into:
         $MODS_DIR
  2. Add to Steam (either):
       - Steam: Add a Non-Steam Game -> browse to $WRAPPER
       - or right-click "Carnivores (Rust rewrite)" in the Desktop Mode
         launcher -> "Add to Steam"
  3. Launch. In the GUI:
       Settings      -> add $MODS_DIR as a mod and add-on folder
       New instance  -> Base game: choose $GAME_DIR; Add-ons: add folders
                        from $MODS_DIR; Save and exit
       Instance box  -> Play
     (The launcher remembers your add-on folder locally; instances keep
      their own saves.)

  Notes:
   - Saves: ~/.local/share/c2rs/ — per instance, survives SteamOS updates
   - Update to the latest commit later (pull + rebuild only):
       curl -fsSL $SCRIPT_URL | bash -s -- --update
     or just re-run the full installer — every step is idempotent.
   - SteamOS updates wipe pacman-installed packages (gcc, alsa-lib, ...) and
     can leave pacman's database claiming they're still installed, or leave
     untracked base-image files blocking reinstalls. The installer detects
     and repairs all of that automatically (sections 2 / 2a: drops stale
     entries, clears stale locks, overwrites only the exact unowned leftover
     files it reports). rustup and the built binaries live in $HOME and
     survive updates.

EOF
