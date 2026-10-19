//! Read-only virtual filesystem exposed over FTP.
//!
//! Layout:
//! ```text
//! /                 -> contains the single directory "Pool"
//! /Pool             -> one entry per pooled item (file or folder)
//! /Pool/<name>      -> the host file, or the root of the host folder
//! /Pool/<name>/...  -> inside the host folder
//! ```

use crate::pool::PoolState;
use async_trait::async_trait;
use std::{
    fmt::Debug,
    io,
    path::{Component, Path, PathBuf},
    pin::Pin,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    task::{Context, Poll},
    time::SystemTime,
};
use tokio::io::{AsyncRead, AsyncSeekExt, ReadBuf};
use unftp_core::auth::UserDetail;
use unftp_core::storage::{Error, ErrorKind, Fileinfo, Metadata, Permissions, Result, StorageBackend};

pub const POOL_DIR: &str = "Pool";

/// Shared transfer counters, sampled by the telemetry loop.
#[derive(Debug, Default)]
pub struct Telemetry {
    /// Bytes served since the last sample (reset by sampler).
    pub window_bytes: AtomicU64,
    /// Bytes served since the server started.
    pub total_bytes: AtomicU64,
    /// Number of data transfers currently open.
    pub active_transfers: AtomicU64,
}

#[derive(Debug, Clone)]
pub struct VMeta {
    len: u64,
    is_dir: bool,
    modified: Option<SystemTime>,
}

impl VMeta {
    fn dir() -> Self {
        VMeta { len: 0, is_dir: true, modified: Some(SystemTime::now()) }
    }
    fn from_fs(m: &std::fs::Metadata) -> Self {
        VMeta { len: m.len(), is_dir: m.is_dir(), modified: m.modified().ok() }
    }
}

impl Metadata for VMeta {
    fn len(&self) -> u64 {
        self.len
    }
    fn is_dir(&self) -> bool {
        self.is_dir
    }
    fn is_file(&self) -> bool {
        !self.is_dir
    }
    fn is_symlink(&self) -> bool {
        false
    }
    fn modified(&self) -> Result<SystemTime> {
        self.modified.ok_or_else(|| Error::from(ErrorKind::LocalError))
    }
    fn gid(&self) -> u32 {
        0
    }
    fn uid(&self) -> u32 {
        0
    }
    fn permissions(&self) -> Permissions {
        // read-only everywhere
        Permissions(if self.is_dir { 0o555 } else { 0o444 })
    }
}

/// Where a virtual path lands.
#[derive(Debug, PartialEq)]
enum Resolved {
    Root,
    PoolRoot,
    Host(PathBuf),
}

fn clean_components(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().to_string()),
            _ => None,
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct PoolFs {
    pool: Arc<PoolState>,
    telemetry: Arc<Telemetry>,
}

impl PoolFs {
    pub fn new(pool: Arc<PoolState>, telemetry: Arc<Telemetry>) -> Self {
        PoolFs { pool, telemetry }
    }

    fn resolve(&self, path: &Path) -> Result<Resolved> {
        let parts = clean_components(path);
        if parts.is_empty() {
            return Ok(Resolved::Root);
        }
        if parts[0] != POOL_DIR {
            return Err(Error::from(ErrorKind::PermanentFileNotAvailable));
        }
        if parts.len() == 1 {
            return Ok(Resolved::PoolRoot);
        }
        let (host, is_dir) = self
            .pool
            .lookup(&parts[1])
            .ok_or_else(|| Error::from(ErrorKind::PermanentFileNotAvailable))?;
        if parts.len() == 2 {
            return Ok(Resolved::Host(host));
        }
        if !is_dir {
            return Err(Error::from(ErrorKind::PermanentDirectoryNotAvailable));
        }
        // Only plain path segments survive clean_components, so no `..` escapes.
        let mut full = host;
        for p in &parts[2..] {
            full.push(p);
        }
        Ok(Resolved::Host(full))
    }

    fn pool_listing(&self) -> Vec<Fileinfo<PathBuf, VMeta>> {
        let mut rows: Vec<_> = self
            .pool
            .entries()
            .into_iter()
            .map(|(name, (host, is_dir, size))| {
                let modified = std::fs::metadata(&host).and_then(|m| m.modified()).ok();
                Fileinfo {
                    path: PathBuf::from(name),
                    metadata: VMeta { len: if is_dir { 0 } else { size }, is_dir, modified },
                }
            })
            .collect();
        rows.sort_by(|a, b| a.path.cmp(&b.path));
        rows
    }
}

#[async_trait]
impl<User: UserDetail> StorageBackend<User> for PoolFs {
    type Metadata = VMeta;

    fn name(&self) -> &str {
        "hotspot-pool-vfs"
    }

    fn supported_features(&self) -> u32 {
        unftp_core::storage::FEATURE_RESTART
    }

    async fn metadata<P: AsRef<Path> + Send + Debug>(&self, _user: &User, path: P) -> Result<VMeta> {
        match self.resolve(path.as_ref())? {
            Resolved::Root | Resolved::PoolRoot => Ok(VMeta::dir()),
            Resolved::Host(p) => {
                let m = tokio::fs::metadata(p).await?;
                Ok(VMeta::from_fs(&m))
            }
        }
    }

    async fn list<P>(&self, _user: &User, path: P) -> Result<Vec<Fileinfo<PathBuf, VMeta>>>
    where
        P: AsRef<Path> + Send + Debug,
    {
        match self.resolve(path.as_ref())? {
            Resolved::Root => Ok(vec![Fileinfo { path: PathBuf::from(POOL_DIR), metadata: VMeta::dir() }]),
            Resolved::PoolRoot => Ok(self.pool_listing()),
            Resolved::Host(dir) => {
                let mut rd = tokio::fs::read_dir(&dir).await?;
                let mut out = Vec::new();
                while let Some(entry) = rd.next_entry().await? {
                    if let Ok(m) = entry.metadata().await {
                        if m.is_file() || m.is_dir() {
                            out.push(Fileinfo {
                                path: PathBuf::from(entry.file_name()),
                                metadata: VMeta::from_fs(&m),
                            });
                        }
                    }
                }
                out.sort_by(|a, b| a.path.cmp(&b.path));
                Ok(out)
            }
        }
    }

    async fn get<P: AsRef<Path> + Send + Debug>(
        &self,
        _user: &User,
        path: P,
        start_pos: u64,
    ) -> Result<Box<dyn AsyncRead + Send + Sync + Unpin>> {
        let host = match self.resolve(path.as_ref())? {
            Resolved::Host(p) => p,
            _ => return Err(Error::from(ErrorKind::PermanentFileNotAvailable)),
        };
        let mut file = tokio::fs::File::open(&host).await?;
        if file.metadata().await?.is_dir() {
            return Err(Error::from(ErrorKind::PermanentFileNotAvailable));
        }
        if start_pos > 0 {
            file.seek(io::SeekFrom::Start(start_pos)).await?;
        }
        self.telemetry.active_transfers.fetch_add(1, Ordering::Relaxed);
        let reader = CountingReader {
            inner: tokio::io::BufReader::with_capacity(256 * 1024, file),
            telemetry: self.telemetry.clone(),
        };
        Ok(Box::new(reader))
    }

    async fn put<P: AsRef<Path> + Send + Debug, R: AsyncRead + Send + Sync + Unpin + 'static>(
        &self,
        _user: &User,
        _input: R,
        _path: P,
        _start_pos: u64,
    ) -> Result<u64> {
        Err(Error::from(ErrorKind::PermissionDenied))
    }

    async fn del<P: AsRef<Path> + Send + Debug>(&self, _user: &User, _path: P) -> Result<()> {
        Err(Error::from(ErrorKind::PermissionDenied))
    }

    async fn mkd<P: AsRef<Path> + Send + Debug>(&self, _user: &User, _path: P) -> Result<()> {
        Err(Error::from(ErrorKind::PermissionDenied))
    }

    async fn rename<P: AsRef<Path> + Send + Debug>(&self, _user: &User, _from: P, _to: P) -> Result<()> {
        Err(Error::from(ErrorKind::PermissionDenied))
    }

    async fn rmd<P: AsRef<Path> + Send + Debug>(&self, _user: &User, _path: P) -> Result<()> {
        Err(Error::from(ErrorKind::PermissionDenied))
    }

    async fn cwd<P: AsRef<Path> + Send + Debug>(&self, user: &User, path: P) -> Result<()> {
        let meta = <Self as StorageBackend<User>>::metadata(self, user, path).await?;
        if meta.is_dir() {
            Ok(())
        } else {
            Err(Error::from(ErrorKind::PermanentDirectoryNotAvailable))
        }
    }
}

/// Wraps a file reader and feeds byte counts into the shared telemetry.
struct CountingReader<R> {
    inner: R,
    telemetry: Arc<Telemetry>,
}

impl<R: AsyncRead + Unpin> AsyncRead for CountingReader<R> {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        let before = buf.filled().len();
        let res = Pin::new(&mut self.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = &res {
            let n = (buf.filled().len() - before) as u64;
            if n > 0 {
                self.telemetry.window_bytes.fetch_add(n, Ordering::Relaxed);
                self.telemetry.total_bytes.fetch_add(n, Ordering::Relaxed);
            }
        }
        res
    }
}

impl<R> Drop for CountingReader<R> {
    fn drop(&mut self) {
        self.telemetry.active_transfers.fetch_sub(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fs_with(items: &[&str]) -> (PoolFs, PathBuf) {
        let dir = std::env::temp_dir().join(format!("hpf-vfs-{}-{}", std::process::id(), items.len()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Videos/sub")).unwrap();
        std::fs::write(dir.join("Videos/a.mp4"), b"0123456789").unwrap();
        std::fs::write(dir.join("Videos/sub/b.mkv"), b"xyz").unwrap();
        std::fs::write(dir.join("notes.txt"), b"hi").unwrap();
        let pool = Arc::new(PoolState::default());
        for it in items {
            pool.add(dir.join(it).to_str().unwrap()).unwrap();
        }
        (PoolFs::new(pool, Arc::new(Telemetry::default())), dir)
    }

    #[test]
    fn resolves_virtual_layout() {
        let (fs, dir) = fs_with(&["Videos", "notes.txt"]);
        assert_eq!(fs.resolve(Path::new("/")).unwrap(), Resolved::Root);
        assert_eq!(fs.resolve(Path::new("/Pool")).unwrap(), Resolved::PoolRoot);
        assert_eq!(fs.resolve(Path::new("/Pool/")).unwrap(), Resolved::PoolRoot);
        let canon = std::fs::canonicalize(&dir).unwrap();
        assert_eq!(fs.resolve(Path::new("/Pool/Videos/sub/b.mkv")).unwrap(), Resolved::Host(canon.join("Videos/sub/b.mkv")));
        assert_eq!(fs.resolve(Path::new("/Pool/notes.txt")).unwrap(), Resolved::Host(canon.join("notes.txt")));
        assert!(fs.resolve(Path::new("/Other")).is_err());
        assert!(fs.resolve(Path::new("/Pool/missing")).is_err());
        // a file cannot have children
        assert!(fs.resolve(Path::new("/Pool/notes.txt/x")).is_err());
        // `..` is stripped, never escapes the item root
        assert_eq!(fs.resolve(Path::new("/Pool/Videos/../../etc/passwd")).unwrap(), Resolved::Host(canon.join("Videos/etc/passwd")));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn lists_and_reads_with_telemetry() {
        use tokio::io::AsyncReadExt;
        use unftp_core::auth::DefaultUser;
        let (fs, dir) = fs_with(&["Videos"]);
        let user = DefaultUser;
        let root = <PoolFs as StorageBackend<DefaultUser>>::list(&fs, &user, "/").await.unwrap();
        assert_eq!(root.len(), 1);
        assert_eq!(root[0].path, PathBuf::from("Pool"));
        let pool = <PoolFs as StorageBackend<DefaultUser>>::list(&fs, &user, "/Pool").await.unwrap();
        assert_eq!(pool.len(), 1);
        assert!(pool[0].metadata.is_dir());
        let inner = <PoolFs as StorageBackend<DefaultUser>>::list(&fs, &user, "/Pool/Videos").await.unwrap();
        assert_eq!(inner.len(), 2);

        let mut r = <PoolFs as StorageBackend<DefaultUser>>::get(&fs, &user, "/Pool/Videos/a.mp4", 4).await.unwrap();
        let mut s = String::new();
        r.read_to_string(&mut s).await.unwrap();
        assert_eq!(s, "456789");
        assert_eq!(fs.telemetry.active_transfers.load(Ordering::Relaxed), 1);
        drop(r);
        assert_eq!(fs.telemetry.active_transfers.load(Ordering::Relaxed), 0);
        assert_eq!(fs.telemetry.total_bytes.load(Ordering::Relaxed), 6);

        assert!(<PoolFs as StorageBackend<DefaultUser>>::del(&fs, &user, "/Pool/Videos/a.mp4").await.is_err());
        assert!(std::fs::metadata(dir.join("Videos/a.mp4")).is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Boots the real libunftp server on loopback with the virtual FS and
    /// drives it with curl: directory listing + ranged download.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn end_to_end_with_curl() {
        if std::process::Command::new("curl").arg("--version").output().is_err() {
            eprintln!("curl not installed; skipping");
            return;
        }
        let (fs, dir) = fs_with(&["Videos", "notes.txt"]);
        let telemetry = fs.telemetry.clone();
        let port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let server = libunftp::ServerBuilder::with_authenticator(
            Box::new(move || fs.clone()),
            Arc::new(libunftp::auth::AnonymousAuthenticator {}),
        )
        .passive_ports(51000..=51010)
        .shutdown_indicator(async move {
            let _ = rx.await;
            libunftp::options::Shutdown::new()
        })
        .build()
        .unwrap();
        let handle = tokio::spawn(async move { server.listen(format!("127.0.0.1:{port}")).await });
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let curl = |args: &[&str]| {
            let out = std::process::Command::new("curl").args(["-s", "-S", "--max-time", "10"]).args(args).output().unwrap();
            assert!(out.status.success(), "curl {:?} failed: {}", args, String::from_utf8_lossy(&out.stderr));
            String::from_utf8_lossy(&out.stdout).to_string()
        };
        let root = curl(&[&format!("ftp://127.0.0.1:{port}/")]);
        assert!(root.contains("Pool"), "root listing: {root}");
        let pool = curl(&[&format!("ftp://127.0.0.1:{port}/Pool/")]);
        assert!(pool.contains("Videos") && pool.contains("notes.txt"), "pool listing: {pool}");
        let inner = curl(&[&format!("ftp://127.0.0.1:{port}/Pool/Videos/")]);
        assert!(inner.contains("a.mp4") && inner.contains("sub"), "inner listing: {inner}");
        let body = curl(&[&format!("ftp://127.0.0.1:{port}/Pool/Videos/sub/b.mkv")]);
        assert_eq!(body, "xyz");
        let ranged = curl(&["-r", "4-", &format!("ftp://127.0.0.1:{port}/Pool/Videos/a.mp4")]);
        assert_eq!(ranged, "456789");
        assert_eq!(telemetry.total_bytes.load(Ordering::Relaxed), 9);
        // writes must be refused
        let up = std::process::Command::new("curl")
            .args(["-s", "--max-time", "10", "-T", dir.join("notes.txt").to_str().unwrap(), &format!("ftp://127.0.0.1:{port}/Pool/Videos/evil.txt")])
            .output()
            .unwrap();
        assert!(!up.status.success());
        assert!(!dir.join("Videos/evil.txt").exists());

        let _ = tx.send(());
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
        let _ = std::fs::remove_dir_all(dir);
    }
}
