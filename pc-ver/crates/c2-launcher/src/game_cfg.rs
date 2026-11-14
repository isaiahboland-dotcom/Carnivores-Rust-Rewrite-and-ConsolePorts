//! Switches in the game's own settings file (`c2rs.cfg` in the game's data
//! folder, `c2-pc/src/settings.rs`) that the launcher also offers, so both
//! show and change the same value: the DHC OS terminal. Only the key
//! changed is touched; every other line is kept as it is.

use std::path::PathBuf;
use std::sync::Mutex;

/// The DHC OS terminal as the main menu and hunt setup (`dhc_terminal`).
pub const DHC_TERMINAL: &str = "dhc_terminal";

fn path() -> PathBuf {
    crate::config::data_dir().join("c2rs.cfg")
}

/// The value of `key` in the file's text (`key = value` lines).
pub fn value(text: &str, key: &str) -> Option<i32> {
    text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == key).then(|| v.trim().parse().unwrap_or(0))
    })
}

/// The text with `key` set to `v` (added at the end when missing).
pub fn with_value(text: &str, key: &str, v: i32) -> String {
    let mut found = false;
    let mut out: Vec<String> = text
        .lines()
        .map(|l| match l.split_once('=') {
            Some((k, _)) if k.trim() == key => {
                found = true;
                format!("{key} = {v}")
            }
            _ => l.to_string(),
        })
        .collect();
    if !found {
        out.push(format!("{key} = {v}"));
    }
    out.join("\n") + "\n"
}

/// Read once, then kept until the launcher changes it or the game closes
/// (the game's own options may have changed it).
static CACHE: Mutex<Option<bool>> = Mutex::new(None);

/// Whether the switch `key` is on (missing file or key: off).
pub fn flag(key: &str) -> bool {
    let mut c = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    *c.get_or_insert_with(|| {
        std::fs::read_to_string(path())
            .ok()
            .and_then(|t| value(&t, key))
            .is_some_and(|v| v != 0)
    })
}

/// Sets the switch `key` in the game's settings file.
pub fn set_flag(key: &str, on: bool) -> Result<(), String> {
    let p = path();
    let text = std::fs::read_to_string(&p).unwrap_or_default();
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&p, with_value(&text, key, i32::from(on)))
        .map_err(|e| format!("{}: {e}", p.display()))?;
    *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = Some(on);
    Ok(())
}

/// Forgets the cached value (the game may have changed the file).
pub fn refresh() {
    *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_is_read_and_set_without_touching_the_rest() {
        let text = "render_distance = 40\nfog = 1\ndhc_terminal = 0\nblood = 1\n";
        assert_eq!(value(text, DHC_TERMINAL), Some(0));
        let on = with_value(text, DHC_TERMINAL, 1);
        assert_eq!(
            on,
            "render_distance = 40\nfog = 1\ndhc_terminal = 1\nblood = 1\n"
        );
        assert_eq!(value(&on, "render_distance"), Some(40));
    }

    #[test]
    fn a_missing_key_or_file_is_off_and_is_added_when_set() {
        assert_eq!(value("fog = 1\n", DHC_TERMINAL), None);
        assert_eq!(
            with_value("fog = 1", DHC_TERMINAL, 1),
            "fog = 1\ndhc_terminal = 1\n"
        );
        assert_eq!(with_value("", DHC_TERMINAL, 1), "dhc_terminal = 1\n");
    }
}
