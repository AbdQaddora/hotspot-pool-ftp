//! Persisted user settings (`settings.json` in the app config dir).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Default suffix. Must be a real top-level domain: phone browsers only treat
/// typed text as an address when it ends in one they know, otherwise they
/// search the web for it.
pub const DEFAULT_SUFFIX: &str = "link";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Short label for the share name: `<name>.<suffix>`.
    pub name: String,
    /// Suffix (a real TLD such as `link`, `site`, `net`).
    pub suffix: String,
    /// Let the Server switch turn the hotspot on/off (Windows).
    pub auto_hotspot: bool,
    /// Accept uploads from phones (into the quarantine inbox).
    pub allow_uploads: bool,
    /// Optional PIN phones must send with uploads. Empty = no PIN.
    pub upload_pin: String,
    /// Largest single upload, in MB.
    pub max_upload_mb: u64,
    /// Total quarantine size allowed, in MB.
    pub inbox_quota_mb: u64,
    /// Inbox folder override; None = app data dir.
    pub inbox_dir: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { name: "share".into(), suffix: DEFAULT_SUFFIX.into(), auto_hotspot: true, allow_uploads: true, upload_pin: String::new(), max_upload_mb: 4096, inbox_quota_mb: 20_480, inbox_dir: None }
    }
}

impl Settings {
    pub fn full_name(&self) -> String {
        format!("{}.{}", self.name, self.suffix)
    }

    pub fn validate_suffix(suffix: &str) -> Result<String, String> {
        let s = suffix.trim().trim_start_matches('.').to_ascii_lowercase();
        if s.len() < 2 || s.len() > 24 || !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') || s.starts_with('-') || s.ends_with('-') {
            return Err("The ending must be 2–24 letters, digits or hyphens (e.g. link, site, net).".into());
        }
        Ok(s)
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
        let s = Settings { name: "abdu".into(), suffix: "link".into(), ..Settings::default() };
        s.save(&dir).unwrap();
        assert_eq!(Settings::load(&dir), s);
        assert_eq!(s.full_name(), "abdu.link");
        assert_eq!(Settings::validate_suffix(".Site ").unwrap(), "site");
        assert!(Settings::validate_suffix("x").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
