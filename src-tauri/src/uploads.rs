//! Upload inbox: files phones send are quarantined here until the laptop
//! user approves them. Approved files move into a "Uploads" folder that is
//! part of the pool.

use crate::pool::PoolState;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

pub const SHARED_FOLDER_NAME: &str = "Uploads";
const EXECUTABLE_EXT: [&str; 20] = ["exe", "msi", "bat", "cmd", "ps1", "vbs", "js", "jse", "wsf", "scr", "com", "pif", "reg", "dll", "jar", "sh", "apk", "app", "dmg", "pkg"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadItem {
    pub id: String,
    /// Sanitized file name (what will be shared).
    pub name: String,
    /// Name exactly as the phone sent it (display only).
    pub original_name: String,
    pub size: u64,
    pub device: String,
    pub ip: String,
    pub received_at: u64,
    pub sha256: String,
    /// Absolute path of the quarantined file.
    pub path: String,
    pub executable: bool,
}

#[derive(Debug)]
pub struct UploadStore {
    inbox: Mutex<PathBuf>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Make a name safe to store on any OS. Never produces path separators.
pub fn sanitize_name(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or(raw);
    let mut s: String = base
        .chars()
        .map(|c| if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') { '_' } else { c })
        .collect();
    s = s.trim().trim_matches('.').trim().to_string();
    if s.is_empty() {
        s = "upload".into();
    }
    // Windows reserved device names (with or without extension).
    let stem = s.split('.').next().unwrap_or("").to_ascii_uppercase();
    const RESERVED: [&str; 22] = ["CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9"];
    if RESERVED.contains(&stem.as_str()) {
        s = format!("_{s}");
    }
    if s.chars().count() > 150 {
        let ext = Path::new(&s).extension().and_then(|e| e.to_str()).map(|e| format!(".{e}")).unwrap_or_default();
        let keep = 150usize.saturating_sub(ext.chars().count());
        s = s.chars().take(keep).collect::<String>() + &ext;
    }
    s
}

pub fn is_executable(name: &str) -> bool {
    Path::new(name).extension().and_then(|e| e.to_str()).map(|e| EXECUTABLE_EXT.contains(&e.to_ascii_lowercase().as_str())).unwrap_or(false)
}

/// `name`, `name (2)`, `name (3)`, ... until unused in `dir`.
fn unique_in(dir: &Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let p = Path::new(name);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or(name);
    let ext = p.extension().and_then(|e| e.to_str()).map(|e| format!(".{e}")).unwrap_or_default();
    (2..).map(|n| dir.join(format!("{stem} ({n}){ext}"))).find(|c| !c.exists()).expect("unbounded")
}

impl UploadStore {
    pub fn new(inbox: PathBuf) -> Self {
        UploadStore { inbox: Mutex::new(inbox) }
    }

    pub fn inbox_dir(&self) -> PathBuf {
        self.inbox.lock().expect("inbox lock").clone()
    }
    pub fn set_inbox_dir(&self, dir: PathBuf) {
        *self.inbox.lock().expect("inbox lock") = dir;
    }
    pub fn shared_dir(&self) -> PathBuf {
        self.inbox_dir().join(SHARED_FOLDER_NAME)
    }
    pub fn part_dir(&self) -> PathBuf {
        self.inbox_dir().join(".part")
    }
    fn meta_path(&self, id: &str) -> PathBuf {
        self.inbox_dir().join(format!("{id}.json"))
    }

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.part_dir())?;
        std::fs::create_dir_all(self.shared_dir())
    }

    /// Bytes currently held in quarantine (+ partial uploads).
    pub fn used_bytes(&self) -> u64 {
        let mut total = 0;
        for dir in [self.inbox_dir(), self.part_dir()] {
            if let Ok(rd) = std::fs::read_dir(dir) {
                for e in rd.flatten() {
                    if e.path().extension().and_then(|x| x.to_str()) == Some("json") {
                        continue; // metadata, not payload
                    }
                    if let Ok(m) = e.metadata() {
                        if m.is_file() {
                            total += m.len();
                        }
                    }
                }
            }
        }
        total
    }

    pub fn new_id() -> String {
        format!("u{:x}{:04x}", now_ms(), rand::random::<u16>())
    }

    /// Register a completed upload (file already at `part_dir/<id>`).
    pub fn commit(&self, id: &str, original_name: &str, size: u64, device: &str, ip: &str, sha256: &str) -> std::io::Result<UploadItem> {
        let name = sanitize_name(original_name);
        let dest = self.inbox_dir().join(format!("{id}_{name}"));
        std::fs::rename(self.part_dir().join(id), &dest)?;
        let item = UploadItem {
            id: id.to_string(),
            name: name.clone(),
            original_name: original_name.chars().take(255).collect(),
            size,
            device: device.chars().take(64).collect(),
            ip: ip.to_string(),
            received_at: now_ms(),
            sha256: sha256.to_string(),
            path: dest.to_string_lossy().to_string(),
            executable: is_executable(&name),
        };
        std::fs::write(self.meta_path(id), serde_json::to_vec_pretty(&item)?)?;
        Ok(item)
    }

    pub fn list(&self) -> Vec<UploadItem> {
        let mut out = Vec::new();
        if let Ok(rd) = std::fs::read_dir(self.inbox_dir()) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) != Some("json") {
                    continue;
                }
                if let Ok(item) = std::fs::read(&p).and_then(|b| serde_json::from_slice::<UploadItem>(&b).map_err(std::io::Error::other)) {
                    if Path::new(&item.path).is_file() {
                        out.push(item);
                    } else {
                        let _ = std::fs::remove_file(&p);
                    }
                }
            }
        }
        out.sort_by(|a, b| b.received_at.cmp(&a.received_at));
        out
    }

    fn get(&self, id: &str) -> Option<UploadItem> {
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
            return None;
        }
        std::fs::read(self.meta_path(id)).ok().and_then(|b| serde_json::from_slice(&b).ok())
    }

    /// Move into the shared folder and make sure that folder is in the pool.
    pub fn approve(&self, id: &str, pool: &PoolState) -> Result<PathBuf, String> {
        let item = self.get(id).ok_or("Upload not found")?;
        std::fs::create_dir_all(self.shared_dir()).map_err(|e| e.to_string())?;
        let dest = unique_in(&self.shared_dir(), &item.name);
        std::fs::rename(&item.path, &dest).or_else(|_| std::fs::copy(&item.path, &dest).map(|_| ()).and_then(|_| std::fs::remove_file(&item.path))).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(self.meta_path(id));
        let shared = self.shared_dir();
        if pool.lookup_by_path(&shared).is_none() {
            pool.add(shared.to_str().ok_or("bad path")?).map_err(|e| e.to_string())?;
        }
        Ok(dest)
    }

    pub fn reject(&self, id: &str) -> Result<(), String> {
        let item = self.get(id).ok_or("Upload not found")?;
        let _ = std::fs::remove_file(&item.path);
        let _ = std::fs::remove_file(self.meta_path(id));
        Ok(())
    }

    /// Remove stale partial uploads (older than one day).
    pub fn sweep_parts(&self) {
        if let Ok(rd) = std::fs::read_dir(self.part_dir()) {
            for e in rd.flatten() {
                if let Ok(m) = e.metadata() {
                    let old = m.modified().ok().and_then(|t| t.elapsed().ok()).map(|d| d.as_secs() > 86_400).unwrap_or(true);
                    if old {
                        let _ = std::fs::remove_file(e.path());
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_sanitized() {
        assert_eq!(sanitize_name("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_name("C:\\Users\\x\\evil.exe"), "evil.exe");
        assert_eq!(sanitize_name("  photo:1?.jpg  "), "photo_1_.jpg");
        assert_eq!(sanitize_name("CON.txt"), "_CON.txt");
        assert_eq!(sanitize_name("..."), "upload");
        assert_eq!(sanitize_name("a\u{0}b.pdf"), "a_b.pdf");
        let long = "x".repeat(300) + ".mp4";
        let s = sanitize_name(&long);
        assert!(s.chars().count() <= 150 && s.ends_with(".mp4"));
        assert!(is_executable("setup.MSI") && is_executable("run.sh") && !is_executable("movie.mkv"));
    }

    #[test]
    fn commit_list_approve_reject() {
        let root = std::env::temp_dir().join(format!("hpf-uploads-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let store = UploadStore::new(root.join("inbox"));
        store.ensure_dirs().unwrap();
        let pool = PoolState::default();

        let id = UploadStore::new_id();
        std::fs::write(store.part_dir().join(&id), b"hello").unwrap();
        let item = store.commit(&id, "../hi:there.txt", 5, "Pixel", "192.168.137.23", "abc").unwrap();
        assert_eq!(item.name, "hi_there.txt");
        assert!(Path::new(&item.path).is_file());
        assert_eq!(store.list().len(), 1);
        assert_eq!(store.used_bytes(), 5);

        let dest = store.approve(&id, &pool).unwrap();
        assert!(dest.starts_with(store.shared_dir()) && dest.is_file());
        assert!(store.list().is_empty());
        assert_eq!(pool.items().len(), 1, "shared folder joined the pool");
        assert_eq!(pool.items()[0].name, SHARED_FOLDER_NAME);

        let id2 = UploadStore::new_id();
        std::fs::write(store.part_dir().join(&id2), b"x").unwrap();
        store.commit(&id2, "hi_there.txt", 1, "Pixel", "ip", "d").unwrap();
        let dest2 = store.approve(&id2, &pool).unwrap();
        assert!(dest2.ends_with("hi_there (2).txt"), "{dest2:?}");

        let id3 = UploadStore::new_id();
        std::fs::write(store.part_dir().join(&id3), b"zz").unwrap();
        let it3 = store.commit(&id3, "junk.bin", 2, "", "ip", "e").unwrap();
        store.reject(&id3).unwrap();
        assert!(!Path::new(&it3.path).exists());
        assert!(store.approve("../x", &pool).is_err());
        let _ = std::fs::remove_dir_all(root);
    }
}
