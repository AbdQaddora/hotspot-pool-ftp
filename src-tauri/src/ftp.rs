//! Embedded FTP server lifecycle, client/transfer events and live telemetry.

use crate::{
    pool::PoolState,
    vfs::{PoolFs, Telemetry},
};
use async_trait::async_trait;
use libunftp::{
    auth::AnonymousAuthenticator,
    notification::{DataEvent, DataListener, EventMeta, PresenceEvent, PresenceListener},
    options::Shutdown,
    ServerBuilder,
};
use serde::Serialize;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;

pub const PREFERRED_PORTS: [u16; 2] = [2121, 2122];
pub const PASSIVE_PORTS: std::ops::RangeInclusive<u16> = 50000..=50100;
const SAMPLE_INTERVAL: Duration = Duration::from_millis(500);

pub const EV_LOG: &str = "ftp_log";
pub const EV_STATS: &str = "ftp_stats";
pub const EV_STATUS: &str = "ftp_status";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FtpStatus {
    pub running: bool,
    pub ip: Option<String>,
    pub port: Option<u16>,
    pub url: Option<String>,
}

impl FtpStatus {
    fn stopped() -> Self {
        FtpStatus { running: false, ip: None, port: None, url: None }
    }
    fn running(addr: SocketAddr) -> Self {
        FtpStatus {
            running: true,
            ip: Some(addr.ip().to_string()),
            port: Some(addr.port()),
            url: Some(format!("ftp://{}:{}/Pool", addr.ip(), addr.port())),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEvent {
    pub ts: u64,
    /// "info" | "success" | "warn" | "error"
    pub level: &'static str,
    /// "server" | "connect" | "disconnect" | "download" | "error"
    pub kind: &'static str,
    pub message: String,
    pub client: Option<String>,
    pub path: Option<String>,
    pub bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsEvent {
    /// Instantaneous download throughput in bytes per second.
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

pub fn emit_log(app: &AppHandle, level: &'static str, kind: &'static str, message: impl Into<String>) {
    let _ = app.emit(
        EV_LOG,
        LogEvent { ts: now_ms(), level, kind, message: message.into(), client: None, path: None, bytes: None },
    );
}

/// Bridges libunftp notifications to Tauri events.
#[derive(Debug, Clone)]
struct EventBridge {
    app: AppHandle,
    connected: Arc<AtomicU64>,
}

#[async_trait]
impl PresenceListener for EventBridge {
    async fn receive_presence_event(&self, e: PresenceEvent, m: EventMeta) {
        let client = Some(m.trace_id.clone());
        let ev = match e {
            PresenceEvent::LoggedIn => {
                let n = self.connected.fetch_add(1, Ordering::Relaxed) + 1;
                LogEvent {
                    ts: now_ms(),
                    level: "success",
                    kind: "connect",
                    message: format!("Client connected ({} online)", n),
                    client,
                    path: None,
                    bytes: None,
                }
            }
            PresenceEvent::LoggedOut => {
                let n = self
                    .connected
                    .try_update(Ordering::Relaxed, Ordering::Relaxed, |v| Some(v.saturating_sub(1)))
                    .map(|v| v.saturating_sub(1))
                    .unwrap_or(0);
                LogEvent {
                    ts: now_ms(),
                    level: "info",
                    kind: "disconnect",
                    message: format!("Client disconnected ({} online)", n),
                    client,
                    path: None,
                    bytes: None,
                }
            }
        };
        let _ = self.app.emit(EV_LOG, ev);
    }
}

#[async_trait]
impl DataListener for EventBridge {
    async fn receive_data_event(&self, e: DataEvent, m: EventMeta) {
        let client = Some(m.trace_id.clone());
        let ev = match e {
            DataEvent::Got { path, bytes } => LogEvent {
                ts: now_ms(),
                level: "success",
                kind: "download",
                message: format!("Downloaded {} ({})", path, human_bytes(bytes)),
                client,
                path: Some(path),
                bytes: Some(bytes),
            },
            other => LogEvent {
                ts: now_ms(),
                level: "warn",
                kind: "error",
                message: format!("Rejected write operation: {:?}", other),
                client,
                path: None,
                bytes: None,
            },
        };
        let _ = self.app.emit(EV_LOG, ev);
    }
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
        format!("{} B", b)
    } else {
        format!("{:.1} {}", v, UNITS[i])
    }
}

struct Running {
    addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    telemetry_task: tauri::async_runtime::JoinHandle<()>,
}

/// Tauri-managed state for the FTP engine.
pub struct FtpEngine {
    pool: Arc<PoolState>,
    telemetry: Arc<Telemetry>,
    connected: Arc<AtomicU64>,
    running: Mutex<Option<Running>>,
}

impl FtpEngine {
    pub fn new(pool: Arc<PoolState>) -> Self {
        FtpEngine {
            pool,
            telemetry: Arc::new(Telemetry::default()),
            connected: Arc::new(AtomicU64::new(0)),
            running: Mutex::new(None),
        }
    }

    pub fn pool(&self) -> &Arc<PoolState> {
        &self.pool
    }

    pub fn status(&self) -> FtpStatus {
        match self.running.lock().expect("ftp lock").as_ref() {
            Some(r) => FtpStatus::running(r.addr),
            None => FtpStatus::stopped(),
        }
    }

    /// Find a free port among the preferred ones (or the explicit one).
    async fn pick_port(ip: IpAddr, explicit: Option<u16>) -> std::result::Result<u16, String> {
        let candidates: Vec<u16> = explicit.map(|p| vec![p]).unwrap_or_else(|| PREFERRED_PORTS.to_vec());
        let mut last_err = String::new();
        for port in candidates {
            match tokio::net::TcpListener::bind(SocketAddr::new(ip, port)).await {
                Ok(l) => {
                    drop(l);
                    return Ok(port);
                }
                Err(e) => last_err = format!("port {port}: {e}"),
            }
        }
        Err(format!("No free port on {ip} ({last_err})"))
    }

    pub async fn start(&self, app: AppHandle, ip: Option<String>, port: Option<u16>) -> std::result::Result<FtpStatus, String> {
        if self.running.lock().expect("ftp lock").is_some() {
            return Ok(self.status());
        }
        let ip: Ipv4Addr = match ip {
            Some(s) => s.parse().map_err(|_| format!("Invalid IPv4 address: {s}"))?,
            None => crate::network::detect_hotspot()
                .and_then(|i| i.ip.parse().ok())
                .ok_or_else(|| "No hotspot network interface detected. Turn on your Mobile Hotspot and try again.".to_string())?,
        };
        let port = Self::pick_port(IpAddr::V4(ip), port).await?;
        let addr = SocketAddr::new(IpAddr::V4(ip), port);

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let bridge = EventBridge { app: app.clone(), connected: self.connected.clone() };
        let pool = self.pool.clone();
        let telemetry = self.telemetry.clone();
        self.connected.store(0, Ordering::Relaxed);
        telemetry.total_bytes.store(0, Ordering::Relaxed);
        telemetry.window_bytes.store(0, Ordering::Relaxed);

        let server = ServerBuilder::with_authenticator(
            Box::new(move || PoolFs::new(pool.clone(), telemetry.clone())),
            Arc::new(AnonymousAuthenticator {}),
        )
        .greeting("Hotspot Pool FTP ready")
        .passive_ports(PASSIVE_PORTS)
        .passive_host(ip)
        .idle_session_timeout(300)
        .notify_presence(bridge.clone())
        .notify_data(bridge)
        .shutdown_indicator(async move {
            let _ = shutdown_rx.await;
            Shutdown::new().grace_period(Duration::from_secs(2))
        })
        .build()
        .map_err(|e| format!("Failed to build FTP server: {e}"))?;

        // Run the listener; report fatal errors back to the UI.
        let app_for_server = app.clone();
        let (ready_tx, ready_rx) = oneshot::channel::<std::result::Result<(), String>>();
        tauri::async_runtime::spawn(async move {
            let listen = server.listen(addr.to_string());
            tokio::pin!(listen);
            // Give the bind a moment to fail fast (e.g. address no longer valid).
            let early = tokio::time::timeout(Duration::from_millis(300), &mut listen).await;
            match early {
                Ok(Err(e)) => {
                    let _ = ready_tx.send(Err(format!("FTP server failed to start: {e}")));
                    return;
                }
                Ok(Ok(())) => {
                    let _ = ready_tx.send(Ok(()));
                    return;
                }
                Err(_elapsed) => {
                    let _ = ready_tx.send(Ok(()));
                }
            }
            if let Err(e) = listen.await {
                emit_log(&app_for_server, "error", "error", format!("FTP server stopped unexpectedly: {e}"));
                let _ = app_for_server.emit(EV_STATUS, FtpStatus::stopped());
            }
        });
        ready_rx.await.map_err(|_| "FTP server task died".to_string())??;

        // Telemetry sampler: speed in bytes/sec, sampled every 500ms.
        let telemetry = self.telemetry.clone();
        let connected = self.connected.clone();
        let app_for_stats = app.clone();
        let telemetry_task = tauri::async_runtime::spawn(async move {
            let mut ticker = tokio::time::interval(SAMPLE_INTERVAL);
            let mut peak = 0f64;
            let mut last = tokio::time::Instant::now();
            loop {
                ticker.tick().await;
                let now = tokio::time::Instant::now();
                let secs = now.duration_since(last).as_secs_f64().max(0.001);
                last = now;
                let bytes = telemetry.window_bytes.swap(0, Ordering::Relaxed);
                let speed = bytes as f64 / secs;
                if speed > peak {
                    peak = speed;
                }
                let _ = app_for_stats.emit(
                    EV_STATS,
                    StatsEvent {
                        speed_bps: speed,
                        peak_bps: peak,
                        connected: connected.load(Ordering::Relaxed),
                        active_transfers: telemetry.active_transfers.load(Ordering::Relaxed),
                        total_bytes: telemetry.total_bytes.load(Ordering::Relaxed),
                    },
                );
            }
        });

        *self.running.lock().expect("ftp lock") = Some(Running { addr, shutdown: Some(shutdown_tx), telemetry_task });
        let status = FtpStatus::running(addr);
        emit_log(&app, "success", "server", format!("FTP server listening on {}", status.url.clone().unwrap_or_default()));
        let _ = app.emit(EV_STATUS, status.clone());
        Ok(status)
    }

    pub async fn stop(&self, app: AppHandle) -> FtpStatus {
        let running = self.running.lock().expect("ftp lock").take();
        if let Some(mut r) = running {
            if let Some(tx) = r.shutdown.take() {
                let _ = tx.send(());
            }
            r.telemetry_task.abort();
            self.connected.store(0, Ordering::Relaxed);
            let _ = app.emit(
                EV_STATS,
                StatsEvent { speed_bps: 0.0, peak_bps: 0.0, connected: 0, active_transfers: 0, total_bytes: self.telemetry.total_bytes.load(Ordering::Relaxed) },
            );
            emit_log(&app, "info", "server", "FTP server stopped");
        }
        let status = FtpStatus::stopped();
        let _ = app.emit(EV_STATUS, status.clone());
        status
    }
}
