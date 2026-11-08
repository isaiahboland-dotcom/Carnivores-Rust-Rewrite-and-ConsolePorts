//! The engine APK under another name and package, so that instances
//! install side by side on one phone (each app has its own files folder).
//!
//! The manifest is `android-ver/AndroidManifest.xml` with the package and
//! label replaced, linked by the SDK's `aapt2`; the engine library is
//! copied over from `c2rs.apk` as it is (stored, as Android maps it from
//! the APK), then the APK is aligned and signed with the debug key, as
//! `android-ver/build-apk.sh` does.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{LABEL, PACKAGE};

const MANIFEST: &str = include_str!("../../../../../android-ver/AndroidManifest.xml");
/// The engine library in the APK (android-ver/build-apk.sh).
const LIB: &str = "lib/arm64-v8a/libc2_android.so";

/// The SDK tools a renamed APK needs.
pub struct Sdk {
    aapt2: PathBuf,
    zipalign: PathBuf,
    apksigner: PathBuf,
    android_jar: PathBuf,
}

/// The newest `build-tools` and platform in the Android SDK.
pub fn find_sdk() -> Result<Sdk, String> {
    let sdk = super::sdk_dir().ok_or("Android SDK not found (set ANDROID_HOME).")?;
    let newest = |dir: &str| -> Result<PathBuf, String> {
        let mut v: Vec<PathBuf> = fs::read_dir(sdk.join(dir))
            .map_err(|_| format!("No {dir} in the Android SDK ({}).", sdk.display()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir())
            .collect();
        v.sort_by_key(|p| version_key(p));
        v.pop()
            .ok_or_else(|| format!("No {dir} in the Android SDK ({}).", sdk.display()))
    };
    let bt = newest("build-tools")?;
    let platform = newest("platforms")?;
    let exe = |n: &str| {
        bt.join(if cfg!(windows) {
            format!("{n}.exe")
        } else {
            n.into()
        })
    };
    let s = Sdk {
        aapt2: exe("aapt2"),
        zipalign: exe("zipalign"),
        apksigner: bt.join(if cfg!(windows) {
            "apksigner.bat"
        } else {
            "apksigner"
        }),
        android_jar: platform.join("android.jar"),
    };
    for p in [&s.aapt2, &s.zipalign, &s.apksigner, &s.android_jar] {
        if !p.is_file() {
            return Err(format!("Missing from the Android SDK: {}", p.display()));
        }
    }
    Ok(s)
}

/// "36.1.0" -> [36, 1, 0]; "android-36.1" -> [36, 1].
fn version_key(p: &Path) -> Vec<u32> {
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    name.split(|c: char| !c.is_ascii_digit())
        .filter_map(|n| n.parse().ok())
        .collect()
}

/// The app's manifest with `label` and `package`.
pub fn manifest(label: &str, package: &str) -> Result<String, String> {
    let swap = |text: &str, from: &str, to: &str| {
        (text.matches(from).count() == 1)
            .then(|| text.replacen(from, to, 1))
            .ok_or_else(|| format!("android-ver/AndroidManifest.xml has no single {from}"))
    };
    let m = swap(
        MANIFEST,
        &format!("package=\"{PACKAGE}\""),
        &format!("package=\"{package}\""),
    )?;
    swap(
        &m,
        &format!("android:label=\"{LABEL}\""),
        &format!("android:label=\"{}\"", xml_escape(label.trim())),
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// `apk` relabelled as `label` with package `package`, built in `work`;
/// returns the signed APK.
pub fn renamed_apk(
    sdk: &Sdk,
    apk: &Path,
    label: &str,
    package: &str,
    work: &Path,
) -> Result<PathBuf, String> {
    let io = |e: std::io::Error| e.to_string();
    if work.exists() {
        fs::remove_dir_all(work).map_err(io)?;
    }
    fs::create_dir_all(work).map_err(io)?;
    let xml = work.join("AndroidManifest.xml");
    fs::write(&xml, manifest(label, package)?).map_err(io)?;
    let unsigned = work.join("unsigned.apk");
    run(Command::new(&sdk.aapt2)
        .arg("link")
        .arg("-o")
        .arg(&unsigned)
        .arg("--manifest")
        .arg(&xml)
        .arg("-I")
        .arg(&sdk.android_jar))?;
    copy_lib(apk, &unsigned)?;
    let aligned = work.join("aligned.apk");
    run(Command::new(&sdk.zipalign)
        .args(["-f", "-P", "16", "4"])
        .arg(&unsigned)
        .arg(&aligned))?;
    let key = debug_key()?;
    let signed = work.join(super::APK);
    run(Command::new(&sdk.apksigner)
        .arg("sign")
        .arg("--ks")
        .arg(&key)
        .args([
            "--ks-pass",
            "pass:android",
            "--key-pass",
            "pass:android",
            "--out",
        ])
        .arg(&signed)
        .arg(&aligned))?;
    Ok(signed)
}

/// The engine library from `apk` into `dest`, entry as it is.
fn copy_lib(apk: &Path, dest: &Path) -> Result<(), String> {
    let z = |e: zip::result::ZipError| format!("{}: {e}", apk.display());
    let mut src =
        zip::ZipArchive::new(fs::File::open(apk).map_err(|e| e.to_string())?).map_err(z)?;
    let lib = src
        .by_name(LIB)
        .map_err(|_| format!("{} has no {LIB}: not the engine APK?", apk.display()))?;
    let out = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(dest)
        .map_err(|e| e.to_string())?;
    let mut w = zip::ZipWriter::new_append(out).map_err(z)?;
    w.raw_copy_file(lib).map_err(z)?;
    w.finish().map_err(z)?;
    Ok(())
}

/// Android's usual debug key (`~/.android/debug.keystore`), made with the
/// JDK's `keytool` if there is none yet; the same key signs every app, so
/// updates install over each other.
fn debug_key() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .ok_or("No home folder for the debug key.")?;
    let key = home.join(".android").join("debug.keystore");
    if key.is_file() {
        return Ok(key);
    }
    fs::create_dir_all(home.join(".android")).map_err(|e| e.to_string())?;
    let keytool = std::env::var_os("JAVA_HOME")
        .map(|j| PathBuf::from(j).join("bin").join("keytool"))
        .filter(|p| p.is_file() || p.with_extension("exe").is_file())
        .unwrap_or_else(|| "keytool".into());
    run(Command::new(keytool)
        .args(["-genkeypair", "-keystore"])
        .arg(&key)
        .args([
            "-storepass",
            "android",
            "-keypass",
            "android",
            "-alias",
            "androiddebugkey",
            "-dname",
            "CN=Android Debug,O=Android,C=US",
            "-keyalg",
            "RSA",
            "-keysize",
            "2048",
            "-validity",
            "10000",
        ]))
    .map_err(|e| format!("No debug key, and keytool (JDK) could not make one: {e}"))?;
    Ok(key)
}

fn run(cmd: &mut Command) -> Result<(), String> {
    let name = Path::new(cmd.get_program())
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let out = cmd.output().map_err(|e| format!("{name}: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{name} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_manifest_takes_the_name_and_package() {
        let m = manifest("Ice Age & <C2>", "io.github.c2rs.ice_age").unwrap_or_default();
        assert!(m.contains("package=\"io.github.c2rs.ice_age\""));
        assert!(m.contains("android:label=\"Ice Age &amp; &lt;C2&gt;\""));
        assert!(!m.contains("package=\"io.github.c2rs\""));
    }

    /// The manifest is bumped with each release: versionName is the
    /// workspace version and versionCode is major*10000 + minor*100 + patch.
    #[test]
    fn the_manifest_version_is_the_workspace_version() {
        let name = env!("CARGO_PKG_VERSION");
        let code = name
            .split('.')
            .map(|n| n.parse::<u32>().unwrap_or(0))
            .fold(0, |acc, n| acc * 100 + n);
        assert!(MANIFEST.contains(&format!("android:versionName=\"{name}\"")));
        assert!(MANIFEST.contains(&format!("android:versionCode=\"{code}\"")));
    }

    #[test]
    fn newest_sdk_folder_by_version() {
        let mut v = [
            PathBuf::from("9.0.0"),
            PathBuf::from("36.1.0"),
            PathBuf::from("36.0.0"),
        ];
        v.sort_by_key(|p| version_key(p));
        assert_eq!(v[2], PathBuf::from("36.1.0"));
        assert!(version_key(Path::new("android-36.1")) > version_key(Path::new("android-36")));
    }

    /// Against the real SDK: `C2RS_APK=path/c2rs.apk C2RS_OUT=dir cargo
    /// test -p c2-launcher -- --ignored renames_a_real_apk`.
    #[test]
    #[ignore = "needs the Android SDK and a built c2rs.apk"]
    fn renames_a_real_apk() -> Result<(), String> {
        let var = |k: &str| {
            std::env::var_os(k)
                .map(PathBuf::from)
                .ok_or(format!("{k} unset"))
        };
        let (apk, out) = (var("C2RS_APK")?, var("C2RS_OUT")?);
        let sdk = find_sdk()?;
        let signed = renamed_apk(&sdk, &apk, "C2 Test", "io.github.c2rs.c2_test", &out)?;
        assert!(signed.is_file());
        Ok(())
    }
}
