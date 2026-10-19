//! Persisted user settings (`settings.json` in the app config dir).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const TLD: &str = "pool";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Short label for the share name: `<name>.pool`.
    pub name: String,
    /// Show the HTTPS link as the primary link.
    pub prefer_https: bool,
    /// Let the Server switch turn the hotspot on/off (Windows).
    pub auto_hotspot: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { name: "share".into(), prefer_https: true, auto_hotspot: true }
    }
}

impl Settings {
    pub fn full_name(&self) -> String {
        format!("{}.{}", self.name, TLD)
    }

    /// Lower-case letters, digits and hyphens; 1–32 chars; no leading/trailing hyphen.
    pub fn validate_label(label: &str) -> Result<String, String> {
        let l = label.trim().to_ascii_lowercase();
        if l.is_empty() || l.len() > 32 {
            return Err("The name must be 1–32 characters.".into());
        }
        if !l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err("Use only letters, digits and hyphens.".into());
        }
        if l.starts_with('-') || l.ends_with('-') {
            return Err("The name cannot start or end with a hyphen.".into());
        }
        Ok(l)
    }

    pub fn load(dir: &Path) -> Settings {
        std::fs::read(dir.join("settings.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let tmp: PathBuf = dir.join("settings.json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, dir.join("settings.json")).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_rules() {
        assert_eq!(Settings::validate_label(" Abdu ").unwrap(), "abdu");
        assert!(Settings::validate_label("").is_err());
        assert!(Settings::validate_label("a b").is_err());
        assert!(Settings::validate_label("-x").is_err());
        assert!(Settings::validate_label("x.y").is_err());
    }

    #[test]
    fn roundtrip() {
        let dir = std::env::temp_dir().join(format!("hpf-settings-{}", std::process::id()));
        let s = Settings { name: "abdu".into(), prefer_https: false, auto_hotspot: true };
        s.save(&dir).unwrap();
        assert_eq!(Settings::load(&dir), s);
        assert_eq!(s.full_name(), "abdu.pool");
        let _ = std::fs::remove_dir_all(dir);
    }
}
