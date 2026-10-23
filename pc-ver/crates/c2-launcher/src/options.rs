//! Engine options as one file to export, keep or share and import again:
//! the balance (prices, scores, and where PC and mobile differ, which one
//! each entry takes), which survivals are offered, the menu style, the PC
//! game's own settings (`c2rs.cfg`) and the 3DS build's choices. No paths
//! (folders differ between machines) and no game data.

use serde::{Deserialize, Serialize};

use crate::config::{Config, MenuStyle, SurvivalModes, Target};

/// The file (`*.c2rs-options.toml`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Options {
    /// A note for whoever reads it.
    pub about: String,
    pub survival: Option<SurvivalModes>,
    pub menu_style: Option<MenuStyle>,
    /// 3DS: the target and the title art.
    pub target_3ds: Option<Target>,
    pub ice_age_title: Option<bool>,
    /// The PC game's settings file, as is (`key = value` lines).
    pub pc_settings: Option<String>,
    /// The balance file, as is (empty: the game's own values).
    pub balance: Option<String>,
}

fn pc_settings_path() -> std::path::PathBuf {
    crate::config::data_dir().join("c2rs.cfg")
}

impl Options {
    /// The options as they are now.
    pub fn current(cfg: &Config, balance: &c2_mods::balance::Balance) -> Self {
        Self {
            about: "c2rs engine options (export from the launcher; import there)".into(),
            survival: Some(cfg.survival_modes),
            menu_style: Some(cfg.menu_style),
            target_3ds: Some(cfg.three_ds.target),
            ice_age_title: Some(cfg.three_ds.ice_age_title),
            pc_settings: std::fs::read_to_string(pc_settings_path()).ok(),
            balance: Some(balance.to_toml()),
        }
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }

    pub fn from_toml(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    /// Puts what the file has into `cfg` and `balance` (what it leaves out
    /// stays) and the PC settings file; returns what changed.
    pub fn apply(
        &self,
        cfg: &mut Config,
        balance: &mut c2_mods::balance::Balance,
    ) -> Result<Vec<&'static str>, String> {
        let mut done = Vec::new();
        if let Some(s) = self.survival {
            cfg.survival_modes = s;
            done.push("survival");
        }
        if let Some(m) = self.menu_style {
            cfg.menu_style = m;
            done.push("menu style");
        }
        if let Some(t) = self.target_3ds {
            cfg.three_ds.target = t;
            done.push("3DS target");
        }
        if let Some(i) = self.ice_age_title {
            cfg.three_ds.ice_age_title = i;
            done.push("3DS title art");
        }
        if let Some(b) = &self.balance {
            *balance = c2_mods::balance::Balance::from_toml(b).map_err(|e| e.to_string())?;
            done.push("balance");
        }
        if let Some(p) = &self.pc_settings {
            let path = pc_settings_path();
            if let Some(d) = path.parent() {
                std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
            }
            std::fs::write(&path, p).map_err(|e| e.to_string())?;
            done.push("PC game settings");
        }
        Ok(done)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_round_trip_and_leave_the_rest() {
        let o = Options {
            about: "x".into(),
            survival: Some(SurvivalModes::Mobile),
            menu_style: Some(MenuStyle::IceAge),
            ..Default::default()
        };
        let back = Options::from_toml(&o.to_toml()).unwrap();
        assert_eq!(back, o);
        let mut cfg = Config::default();
        cfg.three_ds.ice_age_title = true;
        let mut b = c2_mods::balance::Balance::default();
        let done = back.apply(&mut cfg, &mut b).unwrap();
        assert_eq!(done, ["survival", "menu style"]);
        assert_eq!(cfg.survival_modes, SurvivalModes::Mobile);
        assert!(cfg.three_ds.ice_age_title);
    }
}
