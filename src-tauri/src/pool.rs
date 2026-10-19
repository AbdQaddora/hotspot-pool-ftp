//! In-memory "Shared Pool": arbitrary host files and folders mapped into a
//! unified virtual FTP tree under `/Pool/<name>`.

use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        RwLock,
    },
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolItem {
    pub id: String,
    /// Virtual name shown under `/Pool/`.
    pub name: String,
    /// Original absolute host path.
    pub path: String,
    pub is_dir: bool,
    /// Bytes. For folders this is computed asynchronously.
    pub size: u64,
    pub size_pending: bool,
    pub added_at: u64,
}

#[derive(Debug, Default)]
pub struct PoolState {
    items: RwLock<Vec<PoolItem>>,
    next_id: AtomicU64,
}

#[derive(Debug)]
pub enum PoolError {
    NotFound(String),
    Duplicate(String),
}

impl std::error::Error for PoolError {}

impl std::fmt::Display for PoolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PoolError::NotFound(p) => write!(f, "path does not exist: {p}"),
            PoolError::Duplicate(p) => write!(f, "path is already in the pool: {p}"),
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Produce a unique virtual name: `name`, `name (2)`, `name (3)`, ...
fn unique_name(base: &str, taken: &[PoolItem]) -> String {
    let exists = |n: &str| taken.iter().any(|i| i.name.eq_ignore_ascii_case(n));
    if !exists(base) {
        return base.to_string();
    }
    let (stem, ext) = match Path::new(base).extension().and_then(|e| e.to_str()) {
        Some(ext) if !ext.is_empty() => (
            Path::new(base)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(base)
                .to_string(),
            format!(".{ext}"),
        ),
        _ => (base.to_string(), String::new()),
    };
    let mut n = 2u32;
    loop {
        let candidate = format!("{stem} ({n}){ext}");
        if !exists(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

impl PoolState {
    pub fn items(&self) -> Vec<PoolItem> {
        self.items.read().map(|g| g.clone()).unwrap_or_default()
    }

    /// Resolve the host path and directory-ness of a virtual name.
    pub fn lookup(&self, name: &str) -> Option<(PathBuf, bool)> {
        self.items.read().ok().and_then(|g| {
            g.iter()
                .find(|i| i.name == name)
                .map(|i| (PathBuf::from(&i.path), i.is_dir))
        })
    }

    /// Find an item by its canonical host path.
    pub fn lookup_by_path(&self, path: &Path) -> Option<PoolItem> {
        let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let s = canonical.to_string_lossy().to_string();
        self.items.read().ok().and_then(|g| g.iter().find(|i| i.path == s).cloned())
    }

    /// Snapshot of `name -> (path, is_dir)` for listing `/Pool`.
    pub fn entries(&self) -> HashMap<String, (PathBuf, bool, u64)> {
        self.items
            .read()
            .map(|g| {
                g.iter()
                    .map(|i| (i.name.clone(), (PathBuf::from(&i.path), i.is_dir, i.size)))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Add a host path. Returns the new item (with `size_pending` for folders).
    pub fn add(&self, raw: &str) -> Result<PoolItem, PoolError> {
        let path = PathBuf::from(raw);
        let meta = std::fs::metadata(&path).map_err(|_| PoolError::NotFound(raw.to_string()))?;
        let canonical = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        let canonical_str = canonical.to_string_lossy().to_string();

        let mut guard = self.items.write().expect("pool lock poisoned");
        if guard.iter().any(|i| i.path == canonical_str) {
            return Err(PoolError::Duplicate(raw.to_string()));
        }
        let base = canonical
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| !n.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| canonical_str.replace(['\\', '/', ':'], "_"));
        let name = unique_name(&base, &guard);
        let is_dir = meta.is_dir();
        let item = PoolItem {
            id: format!("p{}", self.next_id.fetch_add(1, Ordering::Relaxed) + 1),
            name,
            path: canonical_str,
            is_dir,
            size: if is_dir { 0 } else { meta.len() },
            size_pending: is_dir,
            added_at: now_ms(),
        };
        guard.push(item.clone());
        Ok(item)
    }

    pub fn remove(&self, id: &str) -> bool {
        let mut guard = self.items.write().expect("pool lock poisoned");
        let before = guard.len();
        guard.retain(|i| i.id != id);
        guard.len() != before
    }

    pub fn set_size(&self, id: &str, size: u64) {
        if let Ok(mut guard) = self.items.write() {
            if let Some(item) = guard.iter_mut().find(|i| i.id == id) {
                item.size = size;
                item.size_pending = false;
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct PoolFile {
    paths: Vec<String>,
}

impl PoolState {
    /// Persist the list of host paths (names and sizes are recomputed on load).
    pub fn save(&self, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let data = PoolFile { paths: self.items().into_iter().map(|i| i.path).collect() };
        let tmp = dir.join("pool.json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&data).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, dir.join("pool.json")).map_err(|e| e.to_string())
    }

    /// Re-add the saved paths that still exist. Returns the items that were
    /// restored and the paths that are gone.
    pub fn load(&self, dir: &Path) -> (Vec<PoolItem>, Vec<String>) {
        let Some(data) = std::fs::read(dir.join("pool.json")).ok().and_then(|b| serde_json::from_slice::<PoolFile>(&b).ok()) else {
            return (Vec::new(), Vec::new());
        };
        let mut restored = Vec::new();
        let mut missing = Vec::new();
        for p in data.paths {
            match self.add(&p) {
                Ok(item) => restored.push(item),
                Err(PoolError::NotFound(_)) => missing.push(p),
                Err(PoolError::Duplicate(_)) => {}
            }
        }
        (restored, missing)
    }
}

/// Recursively sum file sizes. Blocking; run on a blocking thread.
pub fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for entry in rd.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_dir() {
                stack.push(entry.path());
            } else if ft.is_file() {
                total += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_lookup_remove_roundtrip() {
        let dir = std::env::temp_dir().join(format!("hpf-pool-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("hello.txt");
        std::fs::write(&f, b"hello").unwrap();

        let pool = PoolState::default();
        let item = pool.add(f.to_str().unwrap()).unwrap();
        assert_eq!(item.name, "hello.txt");
        assert_eq!(item.size, 5);
        assert!(!item.is_dir);
        assert!(pool.lookup("hello.txt").is_some());
        assert!(matches!(pool.add(f.to_str().unwrap()), Err(PoolError::Duplicate(_))));
        assert!(pool.remove(&item.id));
        assert!(pool.lookup("hello.txt").is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn save_and_load_roundtrip_skips_missing() {
        let dir = std::env::temp_dir().join(format!("hpf-poolsave-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("data")).unwrap();
        let f = dir.join("data/keep.txt");
        std::fs::write(&f, b"x").unwrap();
        let gone = dir.join("data/gone.txt");
        std::fs::write(&gone, b"y").unwrap();

        let pool = PoolState::default();
        pool.add(f.to_str().unwrap()).unwrap();
        pool.add(gone.to_str().unwrap()).unwrap();
        pool.save(&dir.join("cfg")).unwrap();
        std::fs::remove_file(&gone).unwrap();

        let fresh = PoolState::default();
        let (restored, missing) = fresh.load(&dir.join("cfg"));
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].name, "keep.txt");
        assert_eq!(missing.len(), 1);
        assert!(missing[0].ends_with("gone.txt"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn duplicate_names_get_suffixed() {
        let a = PoolItem {
            id: "1".into(),
            name: "report.pdf".into(),
            path: "/a/report.pdf".into(),
            is_dir: false,
            size: 0,
            size_pending: false,
            added_at: 0,
        };
        assert_eq!(unique_name("report.pdf", &[a.clone()]), "report (2).pdf");
        assert_eq!(unique_name("Videos", &[a]), "Videos");
    }
}
