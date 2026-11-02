# Windows builds and instance packages

Windows x64 is the initial Windows target. Native gameplay still needs Windows
playtesting; Linux packaging tests do not verify Windows graphics, audio or input.

## Get the engine

The **Windows** GitHub Actions workflow builds and tests the PC engine and launcher
on `windows-latest`. Download its `c2rs-windows-x64` artifact after a successful run.
It contains engine executables only, never game assets. Use builds from the same
revision for the host merger and the Windows executable.

To build locally on Windows, install stable Rust and Visual Studio Build Tools
with **Desktop development with C++** and a Windows SDK. See Rust's
[Windows MSVC requirements](https://doc.rust-lang.org/stable/rustc/platform-support/windows-msvc.html).
In PowerShell, from `pc-ver`:

```powershell
$env:RUSTFLAGS = '-C target-feature=+crt-static'
cargo build --locked --release --target x86_64-pc-windows-msvc -p c2-pc -p c2-launcher
```

The executables are in `target/x86_64-pc-windows-msvc/release`. The CI build also links the C runtime
statically. Start `c2-launcher.exe` and select your own game folders. Saves and
settings use `%APPDATA%\c2rs`. Update graphics drivers if device creation fails.

## Generate a Windows instance in the GUI

1. Put the Windows `c2-pc.exe` in the launcher's **Tools folder** (Settings), or
   beside the launcher. On Linux, also provide the Linux `c2-pc` merger.
2. On **Instances**, choose **Build Windows…** beside your saved instance. The
   launcher checks its sources as it does for console builds.
3. Choose an output parent folder on the **Windows** tab and click
   **Build Windows package**.
4. Copy the entire generated instance folder to Windows and double-click
   `c2-pc.exe`. Keep `game/`, `c2-portable.toml` and the other files beside it.

The GUI packages an existing Windows binary; it does not turn a Linux binary into
an executable or install a cross compiler. Engine updates require an updated
Windows binary and host merger, then another package build. Output choices are
remembered per instance. Save a current unsaved setup as an instance first.

Every build merges the recipe again, copies real files instead of retaining source
symlinks, and includes balance, survival and menu-style overrides. A successful
rebuild replaces the generated instance directory; keep personal files outside it.
A failed copy preserves the previous package. Unrelated output folders are refused.
This is a portable folder, not a single executable containing proprietary assets,
an installer or a signed application. Custom binaries with additional DLL dependencies
must supply those separately; use the CI build for the supported packaging route.

## Optional console sections

In the full launcher's **Settings**, independently turn off **Show the 3DS section**
and **Show the Wii U section**. This hides their tabs, instance build buttons and
tool diagnostics, while retaining saved build preferences. The dedicated console
builders always retain their own platform controls.

## Windows verification before calling it supported

Run the native CI workflow, then on Windows check launcher dialogs and paths with
spaces/Unicode, a packaged game's double-click launch from another working directory,
menus and hunts, audio, mouse capture, controllers, saves, exit/relaunch and rebuilding
an existing instance. Console toolchains remain separately installed dependencies;
the Windows engine artifact does not include devkitPro or console applications.
