//! The active OpenXR runtime (SteamVR, Monado, WiVRn, Oculus ...), found
//! the way the OpenXR loader finds it ("OpenXR Loader - Design and
//! Operation", Runtime Discovery): `XR_RUNTIME_JSON`, else
//! `active_runtime.json` under the XDG config folders and `/etc` (Linux),
//! or the registry's `ActiveRuntime` (Windows). The loader decides in the
//! end; this is for telling the player what is set up, before any headset
//! code runs.

use std::path::{Path, PathBuf};

/// The runtime a manifest names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Runtime {
    /// `runtime.name`, or the manifest's file name if it has none.
    pub name: String,
    pub manifest: PathBuf,
    /// `runtime.library_path`, relative ones taken from the manifest's
    /// folder; `None` if the manifest names none.
    pub library: Option<PathBuf>,
}

impl Runtime {
    /// The library is there (or a bare name the system finds itself).
    pub fn library_found(&self) -> bool {
        match &self.library {
            Some(l) if l.components().count() > 1 => l.is_file(),
            Some(_) => true,
            None => false,
        }
    }
}

/// Where the loader looks, in order (`env` reads environment variables).
pub fn candidates(env: &dyn Fn(&str) -> Option<String>) -> Vec<PathBuf> {
    let set = |k: &str| env(k).filter(|v| !v.is_empty());
    let mut v = Vec::new();
    if let Some(p) = set("XR_RUNTIME_JSON") {
        v.push(PathBuf::from(p));
    }
    if cfg!(windows) {
        return v;
    }
    let file = Path::new("openxr/1/active_runtime.json");
    match set("XDG_CONFIG_HOME") {
        Some(c) => v.push(Path::new(&c).join(file)),
        None => {
            if let Some(h) = set("HOME") {
                v.push(Path::new(&h).join(".config").join(file));
            }
        }
    }
    let dirs = set("XDG_CONFIG_DIRS").unwrap_or_else(|| "/etc/xdg".into());
    for d in dirs.split(':').filter(|d| !d.is_empty()) {
        v.push(Path::new(d).join(file));
    }
    v.push(Path::new("/etc").join(file));
    v
}

/// The first string value of `key` in a JSON text (the manifests are
/// small and flat enough for this; escapes other than `\"` and `\\` are
/// kept as written).
fn json_str(text: &str, key: &str) -> Option<String> {
    let at = text.find(&format!("\"{key}\""))? + key.len() + 2;
    let rest = text[at..].trim_start().strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => out.push(chars.next()?),
            c => out.push(c),
        }
    }
    None
}

/// A runtime manifest read.
pub fn read_manifest(path: &Path) -> Option<Runtime> {
    let text = std::fs::read_to_string(path).ok()?;
    // A relative library is beside the real manifest: `active_runtime.json`
    // is usually a link to it (SteamVR, Monado make one).
    let real = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let library = json_str(&text, "library_path").map(|l| {
        let l = PathBuf::from(l);
        if l.is_relative() && l.components().count() > 1 {
            real.parent().map_or(l.clone(), |d| d.join(&l))
        } else {
            l
        }
    });
    let name = json_str(&text, "name").unwrap_or_else(|| {
        path.file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
    });
    Some(Runtime {
        name,
        manifest: path.to_path_buf(),
        library,
    })
}

/// Windows: the registry's `ActiveRuntime` (read with `reg`, so no
/// registry crate is needed).
fn windows_manifest() -> Option<PathBuf> {
    let out = std::process::Command::new("reg")
        .args([
            "query",
            r"HKLM\SOFTWARE\Khronos\OpenXR\1",
            "/v",
            "ActiveRuntime",
        ])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .find_map(|l| l.split_once("REG_SZ").map(|(_, p)| PathBuf::from(p.trim())))
}

/// The active runtime, or what was looked at when there is none.
pub fn active() -> Result<Runtime, Vec<PathBuf>> {
    let mut looked = candidates(&|k| std::env::var(k).ok());
    if cfg!(windows) {
        if let Some(p) = windows_manifest() {
            looked.push(p);
        }
    }
    looked.iter().find_map(|p| read_manifest(p)).ok_or(looked)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_manifest() {
        let dir = std::env::temp_dir().join(format!("c2-vr-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let m = dir.join("active_runtime.json");
        let _ = std::fs::write(
            &m,
            r#"{ "file_format_version": "1.0.0",
                 "runtime": { "name": "Test \"XR\"", "library_path": "./libtest.so" } }"#,
        );
        let r = read_manifest(&m);
        assert_eq!(r.as_ref().map(|r| r.name.as_str()), Some("Test \"XR\""));
        let real = std::fs::canonicalize(&dir).unwrap_or(dir.clone());
        assert_eq!(r.and_then(|r| r.library), Some(real.join("./libtest.so")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(not(windows))]
    fn search_order_follows_the_loader() {
        let env = |k: &str| match k {
            "HOME" => Some("/home/h".to_string()),
            "XDG_CONFIG_DIRS" => Some("/a:/b".to_string()),
            _ => None,
        };
        let c = candidates(&env);
        let f = "openxr/1/active_runtime.json";
        assert_eq!(
            c,
            vec![
                PathBuf::from("/home/h/.config").join(f),
                PathBuf::from("/a").join(f),
                PathBuf::from("/b").join(f),
                PathBuf::from("/etc").join(f),
            ]
        );
        let env2 = |k: &str| (k == "XR_RUNTIME_JSON").then(|| "/x.json".to_string());
        assert_eq!(candidates(&env2).first(), Some(&PathBuf::from("/x.json")));
    }
}
