//! `.cia` builds of the 3DS app (installed to the home menu with FBI or a
//! similar title manager), made with two tools the user downloads:
//! `makerom` (Project_CTR) and `bannertool`. Each install name gets its own
//! title ID, and its name in the title's RomFS (`name.txt`), which the app
//! reads to find its `sdmc:/3ds/<name>/` folder.
//!
//! The home-menu icon, banner and banner sound are the user's own files;
//! until they are set, the icon and banner are made from a game's own main
//! menu picture on the player's machine ([`game_art`]), else plain
//! placeholders (no artwork in this project).

use std::path::{Path, PathBuf};

use crate::config::Target;
use crate::jobs::Step;

pub const MAKEROM_URL: &str = "https://github.com/3DSGuy/Project_CTR/releases";
pub const BANNERTOOL_URL: &str = "https://github.com/Steveice10/bannertool/releases";

/// Icon and banner sizes the home menu uses.
pub const ICON: (u32, u32) = (48, 48);
pub const BANNER: (u32, u32) = (256, 128);

/// What a `.cia` build needs.
pub struct Cia<'a> {
    /// The install name (`install_name`): title, RomFS name, file name.
    pub name: &'a str,
    pub target: Target,
    pub elf: PathBuf,
    pub makerom: PathBuf,
    pub bannertool: PathBuf,
    /// The user's icon (48x48 PNG), banner (256x128 PNG) and banner sound
    /// (WAV or OGG, up to 3 s); placeholders if not set.
    pub icon: Option<&'a Path>,
    pub banner: Option<&'a Path>,
    pub sound: Option<&'a Path>,
    /// A game folder to take the icon and banner from when they aren't
    /// set ([`game_art`]); placeholders without one.
    pub art_from: Option<&'a Path>,
    /// Where the build's files go.
    pub work: PathBuf,
    pub out: PathBuf,
}

/// A title ID's unique part from the install name (FNV-1a), in
/// 0xF0000-0xFEFFF, away from the system's and the common homebrew IDs.
pub fn unique_id(name: &str) -> u32 {
    let h = name.bytes().fold(0x811c_9dc5u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    });
    0xF0000 + h % 0xF000
}

/// A PNG's size, from its header.
pub fn png_size(path: &Path) -> Option<(u32, u32)> {
    let d = std::fs::read(path).ok()?;
    if d.get(..8)? != b"\x89PNG\r\n\x1a\n" || d.get(12..16)? != b"IHDR" {
        return None;
    }
    let be = |r: std::ops::Range<usize>| {
        d.get(r)
            .and_then(|b| b.try_into().ok())
            .map(u32::from_be_bytes)
    };
    Some((be(16..20)?, be(20..24)?))
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
    }
    !c
}

/// An RGBA8 image as a PNG (stored, uncompressed: placeholders are tiny).
fn png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let mut raw = Vec::new();
    for row in rgba.chunks((w * 4) as usize) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(0xFFFF).collect();
    for (i, b) in blocks.iter().enumerate() {
        z.push(u8::from(i + 1 == blocks.len()));
        let n = b.len() as u16;
        z.extend_from_slice(&n.to_le_bytes());
        z.extend_from_slice(&(!n).to_le_bytes());
        z.extend_from_slice(b);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + u32::from(x)) % 65521;
        b = (b + a) % 65521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |kind: &[u8], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut c = kind.to_vec();
        c.extend_from_slice(data);
        out.extend_from_slice(&c);
        out.extend_from_slice(&crc32(&c).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    out
}

/// A game's TGA (any case, under its game folder) as RGBA8.
fn tga(root: &Path, rel: &str) -> Option<(usize, usize, Vec<u8>)> {
    let p = c2_mods::pack::find_path(root, rel)?;
    let t = c2_formats::tga::parse(&std::fs::read(p).ok()?).ok()?;
    let px = t
        .pixels
        .iter()
        .flat_map(|&v| {
            let c = |s: u16| (((v >> s) & 31) as u32 * 255 / 31) as u8;
            [c(10), c(5), c(0), 255]
        })
        .collect();
    Some((t.width, t.height, px))
}

/// The `(x, y, w, h)` part of an RGBA8 image (`width` wide) scaled to
/// `(tw, th)`, each pixel the mean of the ones it covers.
fn scaled(
    px: &[u8],
    width: usize,
    (x, y, w, h): (usize, usize, usize, usize),
    (tw, th): (u32, u32),
) -> Vec<u8> {
    let (tw, th) = (tw as usize, th as usize);
    let mut out = Vec::with_capacity(tw * th * 4);
    for ty in 0..th {
        let (y0, y1) = (
            y + ty * h / th,
            (y + (ty + 1) * h / th).max(y + ty * h / th + 1),
        );
        for tx in 0..tw {
            let (x0, x1) = (
                x + tx * w / tw,
                (x + (tx + 1) * w / tw).max(x + tx * w / tw + 1),
            );
            let mut sum = [0u32; 4];
            let mut n = 0;
            for sy in y0..y1 {
                for sx in x0..x1 {
                    if let Some(p) = px.get((sy * width + sx) * 4..(sy * width + sx) * 4 + 4) {
                        for (a, &b) in sum.iter_mut().zip(p) {
                            *a += u32::from(b);
                        }
                        n += 1;
                    }
                }
            }
            out.extend(sum.map(|v| (v / n.max(1)) as u8));
        }
    }
    out
}

/// Home-menu art from a game's own main menu picture (`MENU/MENUM.TGA`),
/// made on the player's machine (nothing of it is kept in this project):
/// the banner from its top band at 2:1 (the logo and the menu's dinosaur;
/// below the name / account bar), the icon a square round the menu's
/// dinosaur (on the right, as the menus have it). A game built from the
/// player's APK has no such picture: its art is the phone game's
/// ([`phone_icon`], [`phone_scene`]). `(icon, banner)` as PNGs; `None` if
/// the game has neither.
pub fn game_art(game: &Path) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    let Some((w, h, px)) = menu_picture(game) else {
        let Some(root) = c2_mods::pack::find_game_root(game) else {
            return (None, None);
        };
        return (phone_icon(&root, ICON), phone_scene(&root, BANNER));
    };
    let top = h * 4 / 100;
    let bh = (w / 2).min(h - top);
    let banner = png(BANNER.0, BANNER.1, &scaled(&px, w, (0, top, w, bh), BANNER));
    let icon = png(ICON.0, ICON.1, &scaled(&px, w, icon_square(w, h), ICON));
    (Some(icon), Some(banner))
}

/// One of a phone game's interface pictures (`MOBILE/<name>`, written by
/// the APK base) as RGBA8.
fn phone_png(root: &Path, name: &str) -> Option<(usize, usize, Vec<u8>)> {
    let p = c2_mods::pack::find_path(root, &format!("HUNTDAT/MOBILE/{name}"))?;
    c2_mods::mobile::decode_png(&std::fs::read(p).ok()?)
}

/// A phone game's interface sprite (`MOBILE/SPRITES.CFG`, in its atlas's
/// pixels) as RGBA8.
fn phone_sprite(root: &Path, name: &str) -> Option<(usize, usize, Vec<u8>)> {
    let cfg = c2_mods::pack::find_path(root, "HUNTDAT/MOBILE/SPRITES.CFG")?;
    let mut sprites = std::collections::BTreeMap::new();
    c2_formats::mobile_ui::parse(&std::fs::read_to_string(cfg).ok()?, &mut sprites);
    let s = sprites.get(name)?;
    let (aw, ah, atlas) = phone_png(root, &format!("{}.PNG", s.texture.to_ascii_uppercase()))?;
    let (x, y) = (s.x.round() as usize, s.y.round() as usize);
    let (w, h) = (s.w.round() as usize, s.h.round() as usize);
    if w == 0 || h == 0 || x + w > aw || y + h > ah {
        return None;
    }
    let mut px = Vec::with_capacity(w * h * 4);
    for row in y..y + h {
        px.extend_from_slice(atlas.get((row * aw + x) * 4..(row * aw + x + w) * 4)?);
    }
    Some((w, h, px))
}

/// The phone game's app icon (`MOBILE/ICON.PNG`, its launcher icon) at
/// `size`, as a PNG; else the square round its main menu's animal.
fn phone_icon(root: &Path, size: (u32, u32)) -> Option<Vec<u8>> {
    let (w, h, px) =
        phone_png(root, "ICON.PNG").or_else(|| phone_sprite(root, "menu_main_background"))?;
    let side = w.min(h);
    let x = if w > h { (w - side) * 7 / 10 } else { 0 };
    Some(png(
        size.0,
        size.1,
        &scaled(&px, w, (x, (h - side) / 2, side, side), size),
    ))
}

/// The phone game's main menu at `size` (`menu_main_background`, its
/// middle band cut to the size's shape) with its logo (`menu_main_logo`)
/// over the top left, as on the phone, as a PNG.
fn phone_scene(root: &Path, (tw, th): (u32, u32)) -> Option<Vec<u8>> {
    let (w, h, px) = phone_sprite(root, "menu_main_background")?;
    let bh = (w * th as usize / tw as usize).min(h);
    let mut out = scaled(&px, w, (0, (h - bh) / 3, w, bh), (tw, th));
    if let Some((lw, lh, mut logo)) = phone_sprite(root, "menu_main_logo") {
        // Premultiplied, so the scaled logo's edges don't darken.
        for p in logo.as_chunks_mut::<4>().0 {
            let a = u32::from(p[3]);
            for c in &mut p[..3] {
                *c = (u32::from(*c) * a / 255) as u8;
            }
        }
        let dw = (tw as usize * 11 / 20).max(1);
        let dh = (lh * dw / lw.max(1)).max(1);
        let small = scaled(&logo, lw, (0, 0, lw, lh), (dw as u32, dh as u32));
        let (ox, oy) = (tw as usize / 32, th as usize / 24);
        for y in 0..dh.min(th as usize - oy) {
            for x in 0..dw.min(tw as usize - ox) {
                let s = &small[(y * dw + x) * 4..(y * dw + x) * 4 + 4];
                let i = ((oy + y) * tw as usize + ox + x) * 4;
                let a = u32::from(s[3]);
                for c in 0..3 {
                    out[i + c] =
                        (u32::from(s[c]) + u32::from(out[i + c]) * (255 - a) / 255).min(255) as u8;
                }
            }
        }
    }
    Some(png(tw, th, &out))
}

/// The game's main menu picture (`MENU/MENUM.TGA`) as RGBA8.
fn menu_picture(game: &Path) -> Option<(usize, usize, Vec<u8>)> {
    c2_mods::pack::find_game_root(game).and_then(|root| tga(&root, "HUNTDAT/MENU/MENUM.TGA"))
}

/// The square round the menu's dinosaur (on the right, as the menus have
/// it): `(x, y, side, side)`.
fn icon_square(w: usize, h: usize) -> (usize, usize, usize, usize) {
    let side = (h / 2).min(w);
    let (cx, cy) = (w * 7 / 10, h * 45 / 100);
    let x = cx.saturating_sub(side / 2).min(w - side);
    let y = cy.saturating_sub(side / 2).min(h - side);
    (x, y, side, side)
}

/// The Wii U's home-menu art (`wuhbtool`'s sizes).
pub const WIIU_ICON: (u32, u32) = (128, 128);
pub const WIIU_TV: (u32, u32) = (1280, 720);
pub const WIIU_DRC: (u32, u32) = (854, 480);

/// Wii U home-menu art from the game's main menu picture, as for the 3DS:
/// the icon the square round its dinosaur, the TV's and GamePad's splash
/// screens its 16:9 top band (the logo and the dinosaur); a game built
/// from the player's APK, the phone game's (as [`game_art`]). `[icon, tv,
/// drc]` as PNGs; `None` if the game has no such picture.
pub fn wiiu_art(game: &Path) -> Option<[Vec<u8>; 3]> {
    let Some((w, h, px)) = menu_picture(game) else {
        let root = c2_mods::pack::find_game_root(game)?;
        return Some([
            phone_icon(&root, WIIU_ICON)?,
            phone_scene(&root, WIIU_TV)?,
            phone_scene(&root, WIIU_DRC)?,
        ]);
    };
    let top = h * 4 / 100;
    let band = (0, top, w, (w * 9 / 16).min(h - top));
    Some([
        png(
            WIIU_ICON.0,
            WIIU_ICON.1,
            &scaled(&px, w, icon_square(w, h), WIIU_ICON),
        ),
        png(WIIU_TV.0, WIIU_TV.1, &scaled(&px, w, band, WIIU_TV)),
        png(WIIU_DRC.0, WIIU_DRC.1, &scaled(&px, w, band, WIIU_DRC)),
    ])
}

/// A plain placeholder: the menus' olive with a gold edge.
fn placeholder((w, h): (u32, u32)) -> Vec<u8> {
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let edge = x < 2 || y < 2 || x + 2 >= w || y + 2 >= h;
            px.extend_from_slice(if edge {
                &[240, 200, 70, 255]
            } else {
                &[32, 44, 30, 255]
            });
        }
    }
    png(w, h, &px)
}

/// Half a second of silence (16-bit mono WAV) for the banner.
fn silence() -> Vec<u8> {
    let (rate, n) = (22_050u32, 11_025u32);
    let data = n * 2;
    let mut w = b"RIFF".to_vec();
    w.extend_from_slice(&(36 + data).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&(rate * 2).to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data.to_le_bytes());
    w.resize(w.len() + data as usize, 0);
    w
}

/// The build's steps: placeholders and the spec, the icon and banner
/// (`bannertool`), then the `.cia` (`makerom`) into `out`.
pub fn steps(c: &Cia<'_>) -> Vec<Step> {
    let w = &c.work;
    let s = |p: &Path| p.display().to_string();
    let mut steps = vec![
        Step::Write {
            path: w.join("romfs/name.txt"),
            bytes: c.name.as_bytes().to_vec(),
        },
        Step::Write {
            path: w.join("app.rsf"),
            bytes: RSF.as_bytes().to_vec(),
        },
    ];
    let mut file = |given: Option<&Path>, made: &str, bytes: Vec<u8>| match given {
        Some(p) => p.to_path_buf(),
        None => {
            steps.push(Step::Write {
                path: w.join(made),
                bytes,
            });
            w.join(made)
        }
    };
    let (art_icon, art_banner) = c.art_from.map(game_art).unwrap_or_default();
    let icon = file(
        c.icon,
        "icon.png",
        art_icon.unwrap_or_else(|| placeholder(ICON)),
    );
    let banner = file(
        c.banner,
        "banner.png",
        art_banner.unwrap_or_else(|| placeholder(BANNER)),
    );
    let sound = file(c.sound, "banner.wav", silence());
    let bt = c.bannertool.clone();
    steps.push(Step::Run {
        program: bt.clone(),
        args: vec![
            "makesmdh".into(),
            "-s".into(),
            c.name.into(),
            "-l".into(),
            format!("Carnivores 2 (homebrew) - {}", c.name),
            "-p".into(),
            "c2rs".into(),
            "-i".into(),
            s(&icon),
            "-o".into(),
            s(&w.join("icon.smdh")),
        ],
    });
    steps.push(Step::Run {
        program: bt,
        args: vec![
            "makebanner".into(),
            "-i".into(),
            s(&banner),
            "-a".into(),
            s(&sound),
            "-o".into(),
            s(&w.join("banner.bnr")),
        ],
    });
    // New 3DS builds ask for its larger memory (124 MiB); an Old 3DS one
    // keeps the 64 MiB every console gives.
    let ext = match c.target {
        Target::Old => "Legacy",
        Target::New => "124MB",
    };
    if let Some(d) = c.out.parent() {
        steps.push(Step::Dir(d.to_path_buf()));
    }
    steps.push(Step::Run {
        program: c.makerom.clone(),
        args: vec![
            "-f".into(),
            "cia".into(),
            "-o".into(),
            s(&c.out),
            "-elf".into(),
            s(&c.elf),
            "-rsf".into(),
            s(&w.join("app.rsf")),
            "-icon".into(),
            s(&w.join("icon.smdh")),
            "-banner".into(),
            s(&w.join("banner.bnr")),
            "-exefslogo".into(),
            "-DAPP_TITLE=C2RS".into(),
            "-DAPP_PRODUCT_CODE=CTR-H-C2RS".into(),
            format!("-DAPP_UNIQUE_ID=0x{:05X}", unique_id(c.name)),
            format!("-DAPP_ROMFS={}", s(&w.join("romfs"))),
            format!("-DAPP_SYSTEM_MODE_EXT={ext}"),
        ],
    });
    steps
}

/// The title's spec for `makerom`: the usual homebrew application layout
/// (SD card access, the system services and calls a libctru app uses).
const RSF: &str = r#"BasicInfo:
  Title                   : $(APP_TITLE)
  ProductCode             : $(APP_PRODUCT_CODE)
  Logo                    : Homebrew

RomFs:
  RootPath                : $(APP_ROMFS)

TitleInfo:
  Category                : Application
  UniqueId                : $(APP_UNIQUE_ID)

Option:
  UseOnSD                 : true
  FreeProductCode         : true
  MediaFootPadding        : false
  EnableCrypt             : false
  EnableCompress          : true

AccessControlInfo:
  CoreVersion                   : 2
  DescVersion                   : 2
  ReleaseKernelMajor            : "02"
  ReleaseKernelMinor            : "33"
  UseExtSaveData                : false
  FileSystemAccess:
   - DirectSdmc
   - DirectSdmcWrite
  MemoryType                    : Application
  SystemMode                    : 64MB
  IdealProcessor                : 0
  AffinityMask                  : 1
  Priority                      : 16
  MaxCpu                        : 0x9E
  HandleTableSize               : 0x200
  DisableDebug                  : false
  EnableForceDebug              : false
  CanWriteSharedPage            : true
  CanUsePrivilegedPriority      : false
  CanUseNonAlphabetAndNumber    : true
  PermitMainFunctionArgument    : true
  CanShareDeviceMemory          : true
  RunnableOnSleep               : false
  SpecialMemoryArrange          : true
  SystemModeExt                 : $(APP_SYSTEM_MODE_EXT)
  CpuSpeed                      : 804MHz
  EnableL2Cache                 : true
  CanAccessCore2                : true
  IORegisterMapping:
   - 1ff00000-1ff7ffff
  MemoryMapping:
   - 1f000000-1f5fffff:r
  SystemCallAccess:
    ControlMemory: 1
    QueryMemory: 2
    ExitProcess: 3
    GetProcessAffinityMask: 4
    SetProcessAffinityMask: 5
    GetProcessIdealProcessor: 6
    SetProcessIdealProcessor: 7
    CreateThread: 8
    ExitThread: 9
    SleepThread: 10
    GetThreadPriority: 11
    SetThreadPriority: 12
    GetThreadAffinityMask: 13
    SetThreadAffinityMask: 14
    GetThreadIdealProcessor: 15
    SetThreadIdealProcessor: 16
    GetCurrentProcessorNumber: 17
    Run: 18
    CreateMutex: 19
    ReleaseMutex: 20
    CreateSemaphore: 21
    ReleaseSemaphore: 22
    CreateEvent: 23
    SignalEvent: 24
    ClearEvent: 25
    CreateTimer: 26
    SetTimer: 27
    CancelTimer: 28
    ClearTimer: 29
    CreateMemoryBlock: 30
    MapMemoryBlock: 31
    UnmapMemoryBlock: 32
    CreateAddressArbiter: 33
    ArbitrateAddress: 34
    CloseHandle: 35
    WaitSynchronization1: 36
    WaitSynchronizationN: 37
    SignalAndWait: 38
    DuplicateHandle: 39
    GetSystemTick: 40
    GetHandleInfo: 41
    GetSystemInfo: 42
    GetProcessInfo: 43
    GetThreadInfo: 44
    ConnectToPort: 45
    SendSyncRequest1: 46
    SendSyncRequest2: 47
    SendSyncRequest3: 48
    SendSyncRequest4: 49
    SendSyncRequest: 50
    OpenProcess: 51
    OpenThread: 52
    GetProcessId: 53
    GetProcessIdOfThread: 54
    GetThreadId: 55
    GetResourceLimit: 56
    GetResourceLimitLimitValues: 57
    GetResourceLimitCurrentValues: 58
    GetThreadContext: 59
    Break: 60
    OutputDebugString: 61
    ControlPerformanceCounter: 62
    CreatePort: 71
    CreateSessionToPort: 72
    CreateSession: 73
    AcceptSession: 74
    ReplyAndReceive1: 75
    ReplyAndReceive2: 76
    ReplyAndReceive3: 77
    ReplyAndReceive4: 78
    ReplyAndReceive: 79
    BindInterrupt: 80
    UnbindInterrupt: 81
    InvalidateProcessDataCache: 82
    StoreProcessDataCache: 83
    FlushProcessDataCache: 84
    StartInterProcessDma: 85
    StopDma: 86
    GetDmaState: 87
    RestartDma: 88
    DebugActiveProcess: 96
    BreakDebugProcess: 97
    TerminateDebugProcess: 98
    GetProcessDebugEvent: 99
    ContinueDebugEvent: 100
    GetProcessList: 101
    GetThreadList: 102
    GetDebugThreadContext: 103
    SetDebugThreadContext: 104
    QueryDebugProcessMemory: 105
    ReadProcessMemory: 106
    WriteProcessMemory: 107
    SetHardwareBreakPoint: 108
    GetDebugThreadParam: 109
    ControlProcessMemory: 112
    MapProcessMemory: 113
    UnmapProcessMemory: 114
    CreateCodeSet: 115
    CreateProcess: 117
    TerminateProcess: 118
    SetProcessResourceLimits: 119
    CreateResourceLimit: 120
    SetResourceLimitValues: 121
    AddCodeSegment: 122
    Backdoor: 123
    KernelSetState: 124
    QueryProcessMemory: 125
  ServiceAccessControl:
   - APT:U
   - ac:u
   - am:net
   - boss:U
   - cam:u
   - cecd:u
   - cfg:nor
   - cfg:u
   - csnd:SND
   - dsp::DSP
   - frd:u
   - fs:USER
   - gsp::Gpu
   - gsp::Lcd
   - hid:USER
   - http:C
   - ir:rst
   - ir:u
   - ir:USER
   - mic:u
   - ndm:u
   - news:s
   - nwm::EXT
   - nwm::UDS
   - ptm:sysm
   - ptm:u
   - pxi:dev
   - soc:U
   - ssl:C
   - y2r:u

SystemControlInfo:
  SaveDataSize: 0KB
  RemasterVersion: 0
  StackSize: 0x40000
  Dependency:
    ac: 0x0004013000002402
    am: 0x0004013000001502
    boss: 0x0004013000003402
    camera: 0x0004013000001602
    cecd: 0x0004013000002602
    cfg: 0x0004013000001702
    codec: 0x0004013000001802
    csnd: 0x0004013000002702
    dlp: 0x0004013000002802
    dsp: 0x0004013000001a02
    friends: 0x0004013000003202
    gpio: 0x0004013000001b02
    gsp: 0x0004013000001c02
    hid: 0x0004013000001d02
    http: 0x0004013000002902
    i2c: 0x0004013000001e02
    ir: 0x0004013000003302
    mcu: 0x0004013000001f02
    mic: 0x0004013000002002
    ndm: 0x0004013000002b02
    news: 0x0004013000003502
    nim: 0x0004013000002c02
    nwm: 0x0004013000002d02
    pdn: 0x0004013000002102
    ps: 0x0004013000003102
    ptm: 0x0004013000002202
    ro: 0x0004013000003702
    socket: 0x0004013000002e02
    spi: 0x0004013000002302
    ssl: 0x0004013000002f02
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_differ_by_name_and_stay_in_range() {
        let (a, b) = (unique_id("c2rs"), unique_id("ice-age"));
        assert_ne!(a, b);
        for id in [a, b, unique_id("")] {
            assert!((0xF0000..0xFF000).contains(&id));
        }
    }

    #[test]
    fn a_phone_game_gets_its_icon_and_menu_with_the_logo() {
        let dir = std::env::temp_dir().join(format!("c2rs-cia-phone-{}", std::process::id()));
        let m = dir.join("HUNTDAT/MOBILE");
        let _ = std::fs::create_dir_all(&m);
        let _ = std::fs::write(dir.join("HUNTDAT/_RES.TXT"), "");
        let solid = |w: u32, h: u32, c: [u8; 4]| png(w, h, &c.repeat((w * h) as usize));
        let _ = std::fs::write(m.join("ICON.PNG"), solid(16, 16, [0, 0, 255, 255]));
        let _ = std::fs::write(m.join("MENU_01.PNG"), solid(64, 48, [0, 200, 0, 255]));
        let _ = std::fs::write(m.join("MENU_02.PNG"), solid(32, 8, [255, 0, 0, 255]));
        let _ = std::fs::write(
            m.join("SPRITES.CFG"),
            "sprite menu_main_background {\n\ttexture = \"menu_01\"\n\ttc_x = 0\n\ttc_y = 0\n\t\
             tc_width = 64\n\ttc_height = 48\n}\nsprite menu_main_logo {\n\ttexture = \"menu_02\"\n\t\
             tc_x = 0\n\ttc_y = 0\n\ttc_width = 32\n\ttc_height = 8\n}\n",
        );
        let (icon, banner) = game_art(&dir);
        let decode = |p: Option<Vec<u8>>| p.and_then(|p| c2_mods::mobile::decode_png(&p));
        let (iw, ih, ipx) = decode(icon).unwrap_or_default();
        assert_eq!((iw, ih, &ipx[..4]), (48, 48, &[0, 0, 255, 255][..]));
        let (bw, bh, bpx) = decode(banner).unwrap_or_default();
        assert_eq!((bw, bh), (256, 128));
        // The logo over the top left; the menu below it.
        let at = |x: usize, y: usize| &bpx[(y * bw + x) * 4..(y * bw + x) * 4 + 3];
        assert_eq!(
            (at(20, 10), at(200, 100)),
            (&[255, 0, 0][..], &[0, 200, 0][..])
        );
        assert!(wiiu_art(&dir).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn placeholders_are_pngs_of_the_right_size() {
        let dir = std::env::temp_dir().join(format!("c2rs-cia-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let p = dir.join("icon.png");
        let _ = std::fs::write(&p, placeholder(ICON));
        assert_eq!(png_size(&p), Some(ICON));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
