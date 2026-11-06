# Windows builds and instance packages

Windows x64 is the current Windows build target. CI builds/tests pass. The owner
reports the rewrite running in a Windows VM with 4 vCPUs and 8 GB RAM, with the
original game feeling slightly smoother. The supplied log selects **Microsoft
Basic Render Driver**, Microsoft's WARP software renderer, and reports an
unusable OpenGL 1.1 context (3.3+ required by that backend). The hunt loads and
saves are logged without errors; no FPS or frame-time measurements were captured.
Software rendering makes this unsuitable as a hardware-GPU performance baseline.
See [Microsoft's explanation of BasicRender and WARP](https://learn.microsoft.com/en-us/windows-hardware/drivers/display/microsoft-basic-display-driver).

Excessive mouse sensitivity occurs in both games in that VM; VM input handling
is a hypothesis, not a confirmed engine defect. Physical Windows hardware
gameplay testing remains pending.

## Requirements

| Use | Requirements |
|---|---|
| Run a downloaded build | Windows x64, graphics drivers exposing a backend supported by wgpu, an audio device/driver for sound, your own game folder containing `HUNTDAT`, and the Visual C++ x64 runtime when imported by the executable |
| Build from source | Current stable Rust with the `x86_64-pc-windows-msvc` toolchain, Visual Studio Build Tools with **Desktop development with C++** and a Windows SDK; Git if using the clone command below |
| Build console apps | Separate console toolchains; these are not needed to play on Windows |

The [Rust MSVC target](https://doc.rust-lang.org/stable/rustc/platform-support/windows-msvc.html)
has a Windows 10-or-newer client baseline. This is not a claim that every Windows
version or GPU has been tested with the game. The renderer uses wgpu's compiled
backends (including Direct3D 12 and Vulkan on Windows); a virtual display adapter
may expose fewer capabilities than the host GPU. Install the VM's guest display
drivers/tools and enable 3D acceleration where supported. A working original
Carnivores 2 install alone does not establish support for the rewrite's renderer.

The CI configuration requests static C runtime linking with `+crt-static`.
That setting alone does not establish the dependencies of an arbitrary downloaded
executable: the inspected VM test binary imports `VCRUNTIME140.dll` and Windows
Universal CRT DLLs. Its exact source revision has not been established.
For builds importing the Visual C++ runtime, install Microsoft's supported
[x64 Visual C++ Redistributable](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist).
Use Microsoft's installer rather than downloading individual DLLs from other sites.
A game that already launches is not failing because that runtime is missing.

Rust, build tools, graphics SDKs, devkitPro and original-game wrapper/ReShade DLLs
are not runtime installation steps. Graphics drivers remain required.

## Install the downloaded engine

1. Open the [Windows workflow](https://github.com/isaiahboland-dotcom/Carnivores-Rust-Rewrite-and-ConsolePorts/actions/workflows/windows.yml)
   and choose a successful run for the desired commit. Download its
   **c2rs-windows-x64** artifact; GitHub may require sign-in.
2. Extract the ZIP to a writable folder. Find the executables under
   `pc-ver/target/x86_64-pc-windows-msvc/release/` inside the extraction.
3. Keep `c2-launcher.exe` and `c2-pc.exe` in the same folder. Start
   `c2-launcher.exe`, select your game folder on **The game**, and use **PLAY**.
   The game folder is the parent of `HUNTDAT`, not `HUNTDAT` itself.
4. Saves and settings use `%APPDATA%\c2rs`. Keep that directory when updating the
   executables; game-specific profiles may have their own subfolders there.

The artifact contains no game assets. Use executables from the same revision
when merging and packaging an instance. Console-builder executables in the
artifact do not include the console apps or their toolchains.

To start the engine directly from its executable folder in PowerShell:

```powershell
.\c2-pc.exe play "C:\Games\Carnivores2"
```

For a generated portable instance, double-click its `c2-pc.exe` instead. The
no-argument launch reads `c2-portable.toml` beside the executable; keep that file,
`game/` and the rest of the generated folder together.

## Build from source

Install [Rust using rustup](https://rust-lang.org/tools/install/) and the
[MSVC prerequisites](https://rust-lang.github.io/rustup/installation/windows-msvc.html).
Select **Desktop development with C++**, including its MSVC x64 tools and Windows
SDK, in Visual Studio Installer. Open a fresh PowerShell after installation.

```powershell
git clone https://github.com/isaiahboland-dotcom/Carnivores-Rust-Rewrite-and-ConsolePorts.git c2rs
cd c2rs\pc-ver
rustup toolchain install stable
rustup target add --toolchain stable x86_64-pc-windows-msvc
$env:RUSTFLAGS = '-C target-feature=+crt-static'
cargo +stable build --locked --release --target x86_64-pc-windows-msvc -p c2-pc -p c2-launcher
.\target\x86_64-pc-windows-msvc\release\c2-launcher.exe
```

This matches the Windows CI engine/launcher build. Use **release** executables
for performance comparisons; debug builds are not representative. If the MSVC
linker is not found, verify the C++ workload and use a Developer PowerShell
provided by Visual Studio Build Tools.

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

## Console builds

The full launcher no longer hides the 3DS and Wii U sections: **Build** on an instance asks
for the platform (Windows, 3DS or Wii U) and shows only that builder. Saved build
preferences are kept per instance. The dedicated console builders offer their own
platform only.

## Windows verification before calling it supported

Run the native CI workflow, then on Windows check launcher dialogs and paths with
spaces/Unicode, a packaged game's double-click launch from another working directory,
menus and hunts, audio, mouse capture, controllers, saves, exit/relaunch and rebuilding
an existing instance. Console toolchains remain separately installed dependencies;
the Windows engine artifact does not include devkitPro or console applications.

## Capture a diagnostic log

In PowerShell, open the folder containing the Windows `c2-pc.exe` you tested.
For a generated portable instance, run:

```powershell
$env:RUST_LOG = 'info,wgpu_core=info,wgpu_hal=info,naga=warn'
$env:RUST_BACKTRACE = '1'
.\c2-pc.exe *> .\c2-windows.log
```

For a raw engine download without `c2-portable.toml`, replace the last line with:

```powershell
.\c2-pc.exe play "C:\Games\Carnivores2" *> .\c2-windows.log
```

Reproduce the issue in a hunt for about a minute, then exit normally. The log
captures startup/GPU selection, warnings, errors and any panic backtrace. It is
not a per-frame performance trace and does not record raw mouse deltas. For
performance comparison, record the smoothed FPS shown in the hunt window title,
resolution, map/time of day, render distance, fog, foliage distance, high-end view and active mods.
Use the same scene/settings where possible; the rewrite enables VSync, so FPS
alone may reflect the display's refresh limit rather than available headroom.

Also provide the engine commit or workflow run, Windows version/build,
hypervisor/version, allocated vCPUs/RAM, guest GPU name and driver version,
whether 3D acceleration is enabled, and mouse integration/capture mode. For
mouse issues, note sensitivity settings, mouse DPI if known, whether menus or
only aiming are affected, and whether the original has the same problem.
These fields help separate guest input behavior from engine changes.

Before sharing, redact private paths/usernames and identifiers from logs. Share
GPU/driver names rather than an unredacted full system inventory. Put review
files under `pc-ver/(testing)/windows-vm/` in this checkout (gitignored), or attach
the redacted log to a GitHub issue. Do not include game assets or SDK files.
To clear the diagnostic settings in that PowerShell session afterwards:

```powershell
Remove-Item Env:RUST_LOG, Env:RUST_BACKTRACE -ErrorAction SilentlyContinue
```
