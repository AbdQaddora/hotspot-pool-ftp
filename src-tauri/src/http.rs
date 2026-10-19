//! Embedded HTTP server: serves the mobile web page, JSON listings and file
//! downloads (with HTTP Range support for streaming/resume), and feeds live
//! telemetry + log events to the desktop UI.

use crate::{
    pool::PoolState,
    vpath::{self, Entry, Resolved},
};
use axum::{
    body::Body,
    extract::{ConnectInfo, Path as AxPath, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    pin::Pin,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    task::{Context, Poll},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Runtime};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncSeekExt, ReadBuf},
    sync::oneshot,
};
use tokio_util::io::ReaderStream;

pub const PREFERRED_PORTS: [u16; 2] = [8080, 8081];
const SAMPLE_INTERVAL: Duration = Duration::from_millis(500);
const CLIENT_TTL: Duration = Duration::from_secs(45);
const WEB_PAGE: &str = include_str!("../web/index.html");

pub const EV_LOG: &str = "server_log";
pub const EV_STATS: &str = "server_stats";
pub const EV_STATUS: &str = "server_status";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    pub running: bool,
    pub ip: Option<String>,
    pub port: Option<u16>,
    pub url: Option<String>,
}

impl ServerStatus {
    fn stopped() -> Self {
        ServerStatus { running: false, ip: None, port: None, url: None }
    }
    fn running(addr: SocketAddr) -> Self {
        ServerStatus {
            running: true,
            ip: Some(addr.ip().to_string()),
            port: Some(addr.port()),
            url: Some(format!("http://{}:{}", addr.ip(), addr.port())),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEvent {
    pub ts: u64,
    pub level: &'static str,
    pub kind: &'static str,
    pub message: String,
    pub client: Option<String>,
    pub path: Option<String>,
    pub bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsEvent {
    pub speed_bps: f64,
    pub peak_bps: f64,
    pub connected: u64,
    pub active_transfers: u64,
    pub total_bytes: u64,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn human_bytes(b: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

/// Shared counters sampled by the telemetry loop.
#[derive(Debug, Default)]
pub struct Telemetry {
    pub window_bytes: AtomicU64,
    pub total_bytes: AtomicU64,
    pub active_transfers: AtomicU64,
    clients: Mutex<HashMap<IpAddr, Instant>>,
}

impl Telemetry {
    /// Returns true when this IP is new (or was idle long enough to count as a new visit).
    fn touch(&self, ip: IpAddr) -> bool {
        let mut m = self.clients.lock().expect("clients lock");
        let now = Instant::now();
        let fresh = m.get(&ip).map(|t| now.duration_since(*t) > CLIENT_TTL).unwrap_or(true);
        m.insert(ip, now);
        fresh
    }
    fn connected(&self) -> u64 {
        let mut m = self.clients.lock().expect("clients lock");
        let now = Instant::now();
        m.retain(|_, t| now.duration_since(*t) <= CLIENT_TTL);
        m.len() as u64
    }
    fn reset(&self) {
        self.window_bytes.store(0, Ordering::Relaxed);
        self.total_bytes.store(0, Ordering::Relaxed);
        self.active_transfers.store(0, Ordering::Relaxed);
        self.clients.lock().expect("clients lock").clear();
    }
}

struct Ctx<R: Runtime> {
    app: AppHandle<R>,
    pool: Arc<PoolState>,
    telemetry: Arc<Telemetry>,
}

impl<R: Runtime> Clone for Ctx<R> {
    fn clone(&self) -> Self {
        Ctx { app: self.app.clone(), pool: self.pool.clone(), telemetry: self.telemetry.clone() }
    }
}

impl<R: Runtime> Ctx<R> {
    fn log(&self, level: &'static str, kind: &'static str, message: String, client: Option<String>, path: Option<String>, bytes: Option<u64>) {
        let _ = self.app.emit(EV_LOG, LogEvent { ts: now_ms(), level, kind, message, client, path, bytes });
    }
    fn seen(&self, addr: SocketAddr) {
        if self.telemetry.touch(addr.ip()) {
            let n = self.telemetry.connected();
            self.log("success", "connect", format!("Device {} connected ({n} online)", addr.ip()), Some(addr.ip().to_string()), None, None);
        }
    }
}

pub fn emit_log<R: Runtime>(app: &AppHandle<R>, level: &'static str, kind: &'static str, message: impl Into<String>) {
    let _ = app.emit(EV_LOG, LogEvent { ts: now_ms(), level, kind, message: message.into(), client: None, path: None, bytes: None });
}

// ---------------------------------------------------------------- handlers

async fn page<R: Runtime>(State(ctx): State<Ctx<R>>, ConnectInfo(addr): ConnectInfo<SocketAddr>) -> impl IntoResponse {
    ctx.seen(addr);
    ([(header::CACHE_CONTROL, "no-cache")], Html(WEB_PAGE))
}

#[derive(Deserialize)]
struct ListQuery {
    #[serde(default)]
    path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Listing {
    path: String,
    entries: Vec<Entry>,
}

async fn list<R: Runtime>(State(ctx): State<Ctx<R>>, ConnectInfo(addr): ConnectInfo<SocketAddr>, Query(q): Query<ListQuery>) -> Response {
    ctx.seen(addr);
    let segs = vpath::segments(&q.path);
    let clean = if segs.is_empty() { "/".to_string() } else { format!("/{}", segs.join("/")) };
    match vpath::resolve(&ctx.pool, &q.path) {
        Some(Resolved::Root) => Json(Listing { path: clean, entries: vpath::list_root(&ctx.pool) }).into_response(),
        Some(Resolved::Host(p)) => match vpath::list_dir(&p).await {
            Ok(entries) => Json(Listing { path: clean, entries }).into_response(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => (StatusCode::NOT_FOUND, "not found").into_response(),
            Err(_) => (StatusCode::BAD_REQUEST, "not a folder").into_response(),
        },
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

#[derive(Deserialize)]
struct DlQuery {
    #[serde(default)]
    inline: Option<String>,
}

fn parse_range(h: &HeaderMap, len: u64) -> Result<Option<(u64, u64)>, ()> {
    let Some(v) = h.get(header::RANGE).and_then(|v| v.to_str().ok()) else { return Ok(None) };
    let spec = v.strip_prefix("bytes=").ok_or(())?;
    let (a, b) = spec.split_once('-').ok_or(())?;
    let (start, end) = match (a.trim(), b.trim()) {
        ("", suffix) => {
            let n: u64 = suffix.parse().map_err(|_| ())?;
            (len.saturating_sub(n), len.saturating_sub(1))
        }
        (s, "") => (s.parse().map_err(|_| ())?, len.saturating_sub(1)),
        (s, e) => (s.parse().map_err(|_| ())?, e.parse().map_err(|_| ())?),
    };
    if len == 0 || start >= len || start > end {
        return Err(());
    }
    Ok(Some((start, end.min(len - 1))))
}

async fn download<R: Runtime>(
    State(ctx): State<Ctx<R>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    AxPath(path): AxPath<String>,
    Query(q): Query<DlQuery>,
    headers: HeaderMap,
) -> Response {
    ctx.seen(addr);
    let host = match vpath::resolve(&ctx.pool, &path) {
        Some(Resolved::Host(p)) => p,
        _ => return (StatusCode::NOT_FOUND, "not found").into_response(),
    };
    let meta = match tokio::fs::metadata(&host).await {
        Ok(m) if m.is_file() => m,
        _ => return (StatusCode::NOT_FOUND, "not a file").into_response(),
    };
    let len = meta.len();
    let name = host.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
    let mime = mime_guess::from_path(&host).first_or_octet_stream();

    let range = match parse_range(&headers, len) {
        Ok(r) => r,
        Err(()) => {
            return (StatusCode::RANGE_NOT_SATISFIABLE, [(header::CONTENT_RANGE, format!("bytes */{len}"))]).into_response();
        }
    };
    let (start, end) = range.unwrap_or((0, len.saturating_sub(1)));
    let send_len = if len == 0 { 0 } else { end - start + 1 };

    let mut file = match tokio::fs::File::open(&host).await {
        Ok(f) => f,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "cannot open").into_response(),
    };
    if start > 0 && file.seek(io::SeekFrom::Start(start)).await.is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, "seek failed").into_response();
    }

    let vpath_str = format!("/{}", vpath::segments(&path).join("/"));
    let client = addr.ip().to_string();
    if range.is_none() {
        ctx.log("info", "download", format!("{} started {} ({})", client, name, human_bytes(len)), Some(client.clone()), Some(vpath_str.clone()), Some(len));
    }
    ctx.telemetry.active_transfers.fetch_add(1, Ordering::Relaxed);
    let reader = CountingReader {
        inner: file.take(send_len),
        telemetry: ctx.telemetry.clone(),
        read: 0,
        on_drop: Some(Box::new({
            let ctx = ctx.clone();
            let full = range.is_none();
            move |read| {
                ctx.telemetry.active_transfers.fetch_sub(1, Ordering::Relaxed);
                if full && read >= send_len && send_len > 0 {
                    ctx.log("success", "download", format!("{} downloaded {} ({})", client, name, human_bytes(len)), Some(client.clone()), Some(vpath_str.clone()), Some(len));
                } else if full && read < send_len {
                    ctx.log("warn", "download", format!("{} aborted {} after {}", client, name, human_bytes(read)), Some(client.clone()), Some(vpath_str.clone()), Some(read));
                }
            }
        })),
    };
    let body = Body::from_stream(ReaderStream::with_capacity(reader, 256 * 1024));

    let fname = host.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
    let ascii: String = fname.chars().map(|c| if c.is_ascii_graphic() || c == ' ' { c } else { '_' }).collect::<String>().replace('"', "'");
    let encoded = percent_encoding::utf8_percent_encode(&fname, percent_encoding::NON_ALPHANUMERIC).to_string();
    let disposition = if q.inline.is_some() { "inline" } else { "attachment" };

    let mut resp = Response::builder()
        .status(if range.is_some() { StatusCode::PARTIAL_CONTENT } else { StatusCode::OK })
        .header(header::CONTENT_TYPE, mime.as_ref())
        .header(header::CONTENT_LENGTH, send_len)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::CONTENT_DISPOSITION, format!("{disposition}; filename=\"{ascii}\"; filename*=UTF-8''{encoded}"));
    if range.is_some() {
        resp = resp.header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}"));
    }
    if let Ok(v) = HeaderValue::from_str(&now_ms().to_string()) {
        resp = resp.header("x-served-at", v);
    }
    resp.body(body).unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// Counts streamed bytes for the live speed gauge and reports completion.
struct CountingReader<R> {
    inner: R,
    telemetry: Arc<Telemetry>,
    read: u64,
    on_drop: Option<Box<dyn FnOnce(u64) + Send>>,
}

impl<R: AsyncRead + Unpin> AsyncRead for CountingReader<R> {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        let before = buf.filled().len();
        let res = Pin::new(&mut self.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = &res {
            let n = (buf.filled().len() - before) as u64;
            if n > 0 {
                self.read += n;
                self.telemetry.window_bytes.fetch_add(n, Ordering::Relaxed);
                self.telemetry.total_bytes.fetch_add(n, Ordering::Relaxed);
            }
        }
        res
    }
}

impl<R> Drop for CountingReader<R> {
    fn drop(&mut self) {
        if let Some(f) = self.on_drop.take() {
            f(self.read);
        }
    }
}

// ---------------------------------------------------------------- engine

struct Running {
    addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    telemetry_task: tauri::async_runtime::JoinHandle<()>,
}

pub struct ServerEngine {
    pool: Arc<PoolState>,
    telemetry: Arc<Telemetry>,
    running: Mutex<Option<Running>>,
}

impl ServerEngine {
    pub fn new(pool: Arc<PoolState>) -> Self {
        ServerEngine { pool, telemetry: Arc::new(Telemetry::default()), running: Mutex::new(None) }
    }

    pub fn pool(&self) -> &Arc<PoolState> {
        &self.pool
    }

    pub fn status(&self) -> ServerStatus {
        match self.running.lock().expect("server lock").as_ref() {
            Some(r) => ServerStatus::running(r.addr),
            None => ServerStatus::stopped(),
        }
    }

    pub fn router<R: Runtime>(app: AppHandle<R>, pool: Arc<PoolState>, telemetry: Arc<Telemetry>) -> Router {
        Router::new()
            .route("/", get(page::<R>))
            .route("/api/list", get(list::<R>))
            .route("/dl/{*path}", get(download::<R>))
            .with_state(Ctx { app, pool, telemetry })
    }

    async fn bind(ip: IpAddr, explicit: Option<u16>) -> Result<tokio::net::TcpListener, String> {
        let candidates: Vec<u16> = explicit.map(|p| vec![p]).unwrap_or_else(|| PREFERRED_PORTS.to_vec());
        let mut last = String::new();
        for port in candidates {
            match tokio::net::TcpListener::bind(SocketAddr::new(ip, port)).await {
                Ok(l) => return Ok(l),
                Err(e) => last = format!("port {port}: {e}"),
            }
        }
        Err(format!("No free port on {ip} ({last})"))
    }

    pub async fn start(&self, app: AppHandle, ip: Option<String>, port: Option<u16>) -> Result<ServerStatus, String> {
        if self.running.lock().expect("server lock").is_some() {
            return Ok(self.status());
        }
        let ip: Ipv4Addr = match ip {
            Some(s) => s.parse().map_err(|_| format!("Invalid IPv4 address: {s}"))?,
            None => crate::network::detect_hotspot()
                .and_then(|i| i.ip.parse().ok())
                .ok_or_else(|| "No hotspot network interface detected. Turn on your Mobile Hotspot and try again.".to_string())?,
        };
        let listener = Self::bind(IpAddr::V4(ip), port).await?;
        let addr = listener.local_addr().map_err(|e| e.to_string())?;
        self.telemetry.reset();

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let router = Self::router(app.clone(), self.pool.clone(), self.telemetry.clone());
        let app_for_server = app.clone();
        tauri::async_runtime::spawn(async move {
            let res = axum::serve(listener, router.into_make_service_with_connect_info::<SocketAddr>())
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.await;
                })
                .await;
            if let Err(e) = res {
                emit_log(&app_for_server, "error", "error", format!("HTTP server stopped unexpectedly: {e}"));
                let _ = app_for_server.emit(EV_STATUS, ServerStatus::stopped());
            }
        });

        let telemetry = self.telemetry.clone();
        let app_for_stats = app.clone();
        let telemetry_task = tauri::async_runtime::spawn(async move {
            let mut ticker = tokio::time::interval(SAMPLE_INTERVAL);
            let mut peak = 0f64;
            let mut last = Instant::now();
            loop {
                ticker.tick().await;
                let now = Instant::now();
                let secs = now.duration_since(last).as_secs_f64().max(0.001);
                last = now;
                let speed = telemetry.window_bytes.swap(0, Ordering::Relaxed) as f64 / secs;
                peak = peak.max(speed);
                let _ = app_for_stats.emit(
                    EV_STATS,
                    StatsEvent {
                        speed_bps: speed,
                        peak_bps: peak,
                        connected: telemetry.connected(),
                        active_transfers: telemetry.active_transfers.load(Ordering::Relaxed),
                        total_bytes: telemetry.total_bytes.load(Ordering::Relaxed),
                    },
                );
            }
        });

        *self.running.lock().expect("server lock") = Some(Running { addr, shutdown: Some(shutdown_tx), telemetry_task });
        let status = ServerStatus::running(addr);
        emit_log(&app, "success", "server", format!("Sharing at {}", status.url.clone().unwrap_or_default()));
        let _ = app.emit(EV_STATUS, status.clone());
        Ok(status)
    }

    pub async fn stop(&self, app: AppHandle) -> ServerStatus {
        if let Some(mut r) = self.running.lock().expect("server lock").take() {
            if let Some(tx) = r.shutdown.take() {
                let _ = tx.send(());
            }
            r.telemetry_task.abort();
            let _ = app.emit(EV_STATS, StatsEvent { speed_bps: 0.0, peak_bps: 0.0, connected: 0, active_transfers: 0, total_bytes: self.telemetry.total_bytes.load(Ordering::Relaxed) });
            emit_log(&app, "info", "server", "Server stopped");
        }
        let status = ServerStatus::stopped();
        let _ = app.emit(EV_STATUS, status.clone());
        status
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_parsing() {
        let mut h = HeaderMap::new();
        assert_eq!(parse_range(&h, 100), Ok(None));
        h.insert(header::RANGE, HeaderValue::from_static("bytes=10-19"));
        assert_eq!(parse_range(&h, 100), Ok(Some((10, 19))));
        h.insert(header::RANGE, HeaderValue::from_static("bytes=90-"));
        assert_eq!(parse_range(&h, 100), Ok(Some((90, 99))));
        h.insert(header::RANGE, HeaderValue::from_static("bytes=-5"));
        assert_eq!(parse_range(&h, 100), Ok(Some((95, 99))));
        h.insert(header::RANGE, HeaderValue::from_static("bytes=0-500"));
        assert_eq!(parse_range(&h, 100), Ok(Some((0, 99))));
        h.insert(header::RANGE, HeaderValue::from_static("bytes=200-300"));
        assert_eq!(parse_range(&h, 100), Err(()));
    }

    /// Boots the real axum router on loopback (with a mock Tauri app handle)
    /// and drives it with curl: page, listings, download, range, 404s.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn end_to_end_with_curl() {
        if std::process::Command::new("curl").arg("--version").output().is_err() {
            eprintln!("curl not installed; skipping");
            return;
        }
        let dir = std::env::temp_dir().join(format!("hpf-http-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Videos/sub")).unwrap();
        std::fs::write(dir.join("Videos/a.mp4"), b"0123456789").unwrap();
        std::fs::write(dir.join("Videos/sub/b.mkv"), b"xyz").unwrap();
        std::fs::write(dir.join("notes.txt"), b"hi").unwrap();
        let pool = Arc::new(PoolState::default());
        pool.add(dir.join("Videos").to_str().unwrap()).unwrap();
        pool.add(dir.join("notes.txt").to_str().unwrap()).unwrap();

        let app = tauri::test::mock_app();
        let telemetry = Arc::new(Telemetry::default());
        let router = ServerEngine::router(app.handle().clone(), pool.clone(), telemetry.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = oneshot::channel::<()>();
        let handle = tokio::spawn(async move {
            axum::serve(listener, router.into_make_service_with_connect_info::<SocketAddr>())
                .with_graceful_shutdown(async move {
                    let _ = rx.await;
                })
                .await
                .unwrap();
        });
        tokio::time::sleep(Duration::from_millis(100)).await;

        let curl = |args: &[&str]| {
            let out = std::process::Command::new("curl").args(["-s", "-S", "--max-time", "10"]).args(args).output().unwrap();
            assert!(out.status.success(), "curl {:?} failed: {}", args, String::from_utf8_lossy(&out.stderr));
            String::from_utf8_lossy(&out.stdout).to_string()
        };
        let base = format!("http://127.0.0.1:{port}");

        let page = curl(&[&base]);
        assert!(page.contains("<title>Hotspot Pool</title>"), "page served");

        let root = curl(&[&format!("{base}/api/list?path=/")]);
        assert!(root.contains("\"name\":\"Videos\"") && root.contains("\"isDir\":true"), "root listing: {root}");
        assert!(root.contains("\"name\":\"notes.txt\""), "root listing: {root}");
        let inner = curl(&[&format!("{base}/api/list?path=%2FVideos%2Fsub")]);
        assert!(inner.contains("\"path\":\"/Videos/sub\"") && inner.contains("b.mkv"), "inner listing: {inner}");

        let body = curl(&[&format!("{base}/dl/Videos/sub/b.mkv")]);
        assert_eq!(body, "xyz");
        let headers = curl(&["-I", &format!("{base}/dl/Videos/a.mp4")]);
        assert!(headers.contains("content-disposition: attachment; filename=\"a.mp4\""), "{headers}");
        assert!(headers.contains("accept-ranges: bytes"), "{headers}");
        assert!(headers.to_lowercase().contains("content-type: video/mp4"), "{headers}");
        let ranged = curl(&["-r", "4-6", &format!("{base}/dl/Videos/a.mp4")]);
        assert_eq!(ranged, "456");
        let inline = curl(&["-I", &format!("{base}/dl/Videos/a.mp4?inline=1")]);
        assert!(inline.contains("content-disposition: inline"), "{inline}");

        let status = |url: &str| curl(&["-o", "/dev/null", "-w", "%{http_code}", url]);
        assert_eq!(status(&format!("{base}/dl/missing.bin")), "404");
        assert_eq!(status(&format!("{base}/dl/Videos")), "404");
        assert_eq!(status(&format!("{base}/api/list?path=/nope")), "404");
        assert_eq!(status(&format!("{base}/dl/Videos/../../etc/passwd")), "404");

        // HEAD requests stream no body: b.mkv (3) + range 4-6 of a.mp4 (3)
        assert_eq!(telemetry.total_bytes.load(Ordering::Relaxed), 3 + 3);
        assert_eq!(telemetry.connected(), 1);
        assert_eq!(telemetry.active_transfers.load(Ordering::Relaxed), 0);

        let _ = tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
        let _ = std::fs::remove_dir_all(dir);
    }
}
