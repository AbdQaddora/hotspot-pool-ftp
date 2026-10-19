//! Virtual path resolution for the shared pool.
//!
//! ```text
//! /                 -> one entry per pooled item (file or folder)
//! /<name>           -> the host file, or the root of the host folder
//! /<name>/a/b.mp4   -> inside the host folder
//! ```
//! Only plain path segments are honoured: `.`/`..`/prefixes are dropped, so a
//! request can never escape the pooled folder it starts in.

use crate::pool::PoolState;
use serde::Serialize;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, PartialEq)]
pub enum Resolved {
    Root,
    Host(PathBuf),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    /// Unix millis, if known.
    pub modified: Option<u64>,
}

pub fn segments(path: &str) -> Vec<String> {
    Path::new(path)
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().to_string()),
            _ => None,
        })
        .filter(|s| !s.is_empty())
        .collect()
}

pub fn resolve(pool: &PoolState, path: &str) -> Option<Resolved> {
    let parts = segments(path);
    if parts.is_empty() {
        return Some(Resolved::Root);
    }
    let (host, is_dir) = pool.lookup(&parts[0])?;
    if parts.len() == 1 {
        return Some(Resolved::Host(host));
    }
    if !is_dir {
        return None;
    }
    let mut full = host;
    for p in &parts[1..] {
        full.push(p);
    }
    Some(Resolved::Host(full))
}

fn millis(t: std::io::Result<std::time::SystemTime>) -> Option<u64> {
    t.ok()?.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_millis() as u64)
}

/// List the pool root.
pub fn list_root(pool: &PoolState) -> Vec<Entry> {
    let mut out: Vec<Entry> = pool
        .entries()
        .into_iter()
        .map(|(name, (host, is_dir, size))| Entry {
            name,
            is_dir,
            size: if is_dir { size } else { std::fs::metadata(&host).map(|m| m.len()).unwrap_or(size) },
            modified: std::fs::metadata(&host).ok().and_then(|m| millis(m.modified())),
        })
        .collect();
    sort(&mut out);
    out
}

/// List a host directory (folders first, then files, case-insensitive).
pub async fn list_dir(dir: &Path) -> std::io::Result<Vec<Entry>> {
    let mut rd = tokio::fs::read_dir(dir).await?;
    let mut out = Vec::new();
    while let Some(e) = rd.next_entry().await? {
        let Ok(m) = e.metadata().await else { continue };
        if !(m.is_file() || m.is_dir()) {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        out.push(Entry { name, is_dir: m.is_dir(), size: if m.is_dir() { 0 } else { m.len() }, modified: millis(m.modified()) });
    }
    sort(&mut out);
    Ok(out)
}

fn sort(v: &mut [Entry]) {
    v.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn fixture(tag: &str) -> (Arc<PoolState>, PathBuf) {
        let dir = std::env::temp_dir().join(format!("hpf-vpath-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Videos/sub")).unwrap();
        std::fs::write(dir.join("Videos/a.mp4"), b"0123456789").unwrap();
        std::fs::write(dir.join("Videos/sub/b.mkv"), b"xyz").unwrap();
        std::fs::write(dir.join("notes.txt"), b"hi").unwrap();
        let pool = Arc::new(PoolState::default());
        pool.add(dir.join("Videos").to_str().unwrap()).unwrap();
        pool.add(dir.join("notes.txt").to_str().unwrap()).unwrap();
        (pool, std::fs::canonicalize(dir).unwrap())
    }

    #[test]
    fn resolves_and_blocks_escapes() {
        let (pool, dir) = fixture("resolve");
        assert_eq!(resolve(&pool, "/").unwrap(), Resolved::Root);
        assert_eq!(resolve(&pool, "").unwrap(), Resolved::Root);
        assert_eq!(resolve(&pool, "/Videos/sub/b.mkv").unwrap(), Resolved::Host(dir.join("Videos/sub/b.mkv")));
        assert_eq!(resolve(&pool, "notes.txt").unwrap(), Resolved::Host(dir.join("notes.txt")));
        assert!(resolve(&pool, "/missing").is_none());
        assert!(resolve(&pool, "/notes.txt/x").is_none());
        assert_eq!(resolve(&pool, "/Videos/../../etc/passwd").unwrap(), Resolved::Host(dir.join("Videos/etc/passwd")));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn lists_root_and_dirs() {
        let (pool, dir) = fixture("list");
        let root = list_root(&pool);
        assert_eq!(root.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), vec!["Videos", "notes.txt"]);
        let inner = list_dir(&dir.join("Videos")).await.unwrap();
        assert_eq!(inner.iter().map(|e| (e.name.as_str(), e.is_dir)).collect::<Vec<_>>(), vec![("sub", true), ("a.mp4", false)]);
        let _ = std::fs::remove_dir_all(dir);
    }
}
