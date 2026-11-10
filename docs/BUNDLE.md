# Desktop bundles

Extract the entire archive into a writable folder. Keep the executables and
`recipes/` together. Start `c2-launcher.exe` on Windows, or `./c2-launcher` on
Linux. Create a **New instance**, choose your own game folder containing
`HUNTDAT`, save, and press **Play**. No game or mod data is included.
Saves and settings use the normal per-user c2rs data directory.

The host tools include the launcher, PC engine, experimental VR engine, cooker,
and 3DS/Wii U builder interfaces. Rust and C++ compilers are not needed to play.
Windows builds use the static MSVC runtime. Linux builds target x86-64 Ubuntu
22.04 or newer (glibc 2.35); install the distribution's ALSA, udev, X11/Wayland,
Vulkan/OpenGL drivers and desktop file picker support. Ubuntu:

```sh
sudo apt install libasound2 libudev1 libx11-6 libxcursor1 libxi6 libxrandr2 libxkbcommon0 libwayland-client0 libegl1 libgl1 libvulkan1 xdg-desktop-portal
```

VR additionally needs an installed OpenXR runtime. Console packaging requires
separately built console applications and third-party tools: `c2-3ds.3dsx` (and
`c2-3ds.elf` for CIA), `makerom`/`bannertool` for CIA, or
`c2-wiiu-perf-probe.rpx` and `wuhbtool` for Wii U. Set their folder under
**Settings > tools**. These console payloads/toolchains are not in the desktop
bundle; see the included platform guides. Android requires its separately built
APK and Android SDK tools. The host builder executables alone do not supply these.

`manifest.json` records the build revision and SHA-256 of every packaged file.
Recipes contain credits, download links and hashes; download their content from
the authors and select your own base game. The archive contains no `source/`,
personal instances, game data or mod data.
