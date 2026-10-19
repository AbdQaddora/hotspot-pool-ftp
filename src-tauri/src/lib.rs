mod elevate;
mod hotspot;
mod http;
mod ics;
mod names;
mod network;
mod pool;
mod settings;
mod uploads;
mod vpath;

use hotspot::HotspotInfo;
use http::{ServerEngine, ServerStatus};
use serde::Serialize;
use settings::Settings;
use std::time::Duration;
use uploads::{UploadItem, UploadStore};
use network::NetInterface;
use pool::{PoolItem, PoolState};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

const EV_POOL: &str = "pool_changed";

#[tauri::command]
fn get_network_interfaces() -> Vec<NetInterface> {
    network::list_interfaces()
}

#[tauri::command]
fn get_hotspot_interface() -> Option<NetInterface> {
    network::detect_hotspot()
}

#[tauri::command]
async fn start_server(app: AppHandle, engine: State<'_, Arc<ServerEngine>>, ip: Option<String>, port: Option<u16>) -> Result<ServerStatus, String> {
    engine.start(app, ip, port).await
}

#[tauri::command]
async fn stop_server(app: AppHandle, engine: State<'_, Arc<ServerEngine>>) -> Result<ServerStatus, String> {
    Ok(engine.stop(app).await)
}

#[tauri::command]
fn get_server_status(engine: State<'_, Arc<ServerEngine>>) -> ServerStatus {
    engine.status()
}

#[tauri::command]
fn get_pool(engine: State<'_, Arc<ServerEngine>>) -> Vec<PoolItem> {
    engine.pool().items()
}

fn broadcast_pool(app: &AppHandle, pool: &PoolState) {
    let _ = app.emit(EV_POOL, pool.items());
    if let Ok(dir) = config_dir(app) {
        if let Err(e) = pool.save(&dir) {
            http::emit_log(app, "warn", "server", format!("Could not save the pool list: {e}"));
        }
    }
}

/// Adds one or more host paths. Folder sizes are computed in the background
/// and pushed via a `pool_changed` event when ready.
#[tauri::command]
fn add_path_to_pool(app: AppHandle, engine: State<'_, Arc<ServerEngine>>, paths: Vec<String>) -> Result<Vec<PoolItem>, String> {
    let pool = engine.pool().clone();
    let mut errors = Vec::new();
    for raw in paths {
        match pool.add(&raw) {
            Ok(item) => {
                http::emit_log(&app, "info", "server", format!("Added {} to pool", item.name));
                if item.is_dir {
                    let app = app.clone();
                    let pool = pool.clone();
                    let id = item.id.clone();
                    let path = std::path::PathBuf::from(&item.path);
                    tauri::async_runtime::spawn_blocking(move || {
                        let size = pool::dir_size(&path);
                        pool.set_size(&id, size);
                        broadcast_pool(&app, &pool);
                    });
                }
            }
            Err(e) => errors.push(e.to_string()),
        }
    }
    broadcast_pool(&app, &pool);
    if errors.is_empty() {
        Ok(pool.items())
    } else {
        Err(errors.join("\n"))
    }
}

/// Set or clear the password of a pooled item (None/empty clears).
#[tauri::command]
fn set_pool_password(app: AppHandle, engine: State<'_, Arc<ServerEngine>>, id: String, password: Option<String>) -> Result<Vec<PoolItem>, String> {
    let pw = password.as_deref().map(str::trim).filter(|p| !p.is_empty());
    let item = engine.pool().set_password(&id, pw)?;
    http::emit_log(&app, "info", "server", if pw.is_some() { format!("Password set on {}", item.name) } else { format!("Password removed from {}", item.name) });
    broadcast_pool(&app, engine.pool());
    Ok(engine.pool().items())
}

#[tauri::command]
fn remove_path_from_pool(app: AppHandle, engine: State<'_, Arc<ServerEngine>>, id: String) -> Vec<PoolItem> {
    let pool = engine.pool();
    if pool.remove(&id) {
        http::emit_log(&app, "info", "server", "Removed item from pool");
    }
    broadcast_pool(&app, pool);
    pool.items()
}

#[tauri::command]
fn get_settings(engine: State<'_, Arc<ServerEngine>>) -> Settings {
    engine.settings()
}

/// Save settings. When the share name changes while sharing, re-run the name setup.
#[tauri::command]
async fn set_settings(app: AppHandle, engine: State<'_, Arc<ServerEngine>>, settings: Settings) -> Result<Settings, String> {
    let name = Settings::validate_label(&settings.name)?;
    let suffix = Settings::validate_suffix(&settings.suffix)?;
    let upload_pin = settings.upload_pin.trim().to_string();
    if !upload_pin.is_empty() && (upload_pin.len() < 4 || upload_pin.len() > 32) {
        return Err("The upload PIN must be 4–32 characters.".into());
    }
    let max_upload_mb = settings.max_upload_mb.clamp(1, 1024 * 1024);
    let inbox_quota_mb = settings.inbox_quota_mb.clamp(max_upload_mb, 1024 * 1024 * 10);
    let inbox_dir = settings.inbox_dir.as_deref().map(str::trim).filter(|d| !d.is_empty()).map(String::from);
    if let Some(d) = &inbox_dir {
        std::fs::create_dir_all(d).map_err(|e| format!("Inbox folder is not usable: {e}"))?;
    }
    let next = Settings { name, suffix, upload_pin, max_upload_mb, inbox_quota_mb, inbox_dir: inbox_dir.clone(), ..settings };
    next.save(&config_dir(&app)?)?;
    engine.uploads().set_inbox_dir(inbox_dir.map(std::path::PathBuf::from).unwrap_or_else(|| default_inbox_dir(&app)));
    let _ = engine.uploads().ensure_dirs();
    let name_changed = engine.set_settings(next.clone());
    if engine.status().running {
        if name_changed {
            if let Err(e) = engine.setup_name(&app).await {
                http::emit_log(&app, "warn", "server", e);
            }
        } else {
            let _ = app.emit(http::EV_STATUS, engine.status());
        }
    }
    Ok(next)
}

#[tauri::command]
async fn setup_name(app: AppHandle, engine: State<'_, Arc<ServerEngine>>) -> Result<ServerStatus, String> {
    engine.setup_name(&app).await
}

#[tauri::command]
async fn check_name(app: AppHandle, engine: State<'_, Arc<ServerEngine>>) -> Result<ServerStatus, String> {
    Ok(engine.verify_name(&app).await)
}

fn config_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path().app_config_dir().map_err(|e| e.to_string())
}

fn default_inbox_dir<R: tauri::Runtime>(app: &AppHandle<R>) -> std::path::PathBuf {
    app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir().join("hotspot-pool")).join("inbox")
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct InboxInfo {
    dir: String,
    used_bytes: u64,
    quota_bytes: u64,
    pending: usize,
}

#[tauri::command]
fn list_uploads(engine: State<'_, Arc<ServerEngine>>) -> Vec<UploadItem> {
    engine.uploads().list()
}

#[tauri::command]
fn get_inbox_info(engine: State<'_, Arc<ServerEngine>>) -> InboxInfo {
    let s = engine.settings();
    InboxInfo { dir: engine.uploads().inbox_dir().to_string_lossy().to_string(), used_bytes: engine.uploads().used_bytes(), quota_bytes: s.inbox_quota_mb.saturating_mul(1024 * 1024), pending: engine.uploads().list().len() }
}

#[tauri::command]
fn approve_upload(app: AppHandle, engine: State<'_, Arc<ServerEngine>>, id: String) -> Result<Vec<UploadItem>, String> {
    let dest = engine.uploads().approve(&id, engine.pool())?;
    http::emit_log(&app, "success", "upload", format!("Approved {} → shared in /{}", dest.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), uploads::SHARED_FOLDER_NAME));
    broadcast_pool(&app, engine.pool());
    Ok(engine.broadcast_uploads(&app))
}

#[tauri::command]
fn reject_upload(app: AppHandle, engine: State<'_, Arc<ServerEngine>>, id: String) -> Result<Vec<UploadItem>, String> {
    engine.uploads().reject(&id)?;
    http::emit_log(&app, "info", "upload", "Rejected an upload");
    Ok(engine.broadcast_uploads(&app))
}

#[tauri::command]
fn approve_all_uploads(app: AppHandle, engine: State<'_, Arc<ServerEngine>>) -> Result<Vec<UploadItem>, String> {
    let mut n = 0;
    for item in engine.uploads().list() {
        if engine.uploads().approve(&item.id, engine.pool()).is_ok() {
            n += 1;
        }
    }
    http::emit_log(&app, "success", "upload", format!("Approved {n} upload(s)"));
    broadcast_pool(&app, engine.pool());
    Ok(engine.broadcast_uploads(&app))
}

#[tauri::command]
fn reject_all_uploads(app: AppHandle, engine: State<'_, Arc<ServerEngine>>) -> Result<Vec<UploadItem>, String> {
    let mut n = 0;
    for item in engine.uploads().list() {
        if engine.uploads().reject(&item.id).is_ok() {
            n += 1;
        }
    }
    http::emit_log(&app, "info", "upload", format!("Rejected {n} upload(s)"));
    Ok(engine.broadcast_uploads(&app))
}

#[tauri::command]
async fn get_hotspot_info() -> HotspotInfo {
    tauri::async_runtime::spawn_blocking(hotspot::info).await.unwrap_or_else(|e| HotspotInfo::unsupported(e.to_string()))
}

#[tauri::command]
async fn set_hotspot(app: AppHandle, on: bool) -> Result<HotspotInfo, String> {
    http::emit_log(&app, "info", "server", if on { "Turning on Mobile Hotspot…" } else { "Turning off Mobile Hotspot…" });
    let info = tauri::async_runtime::spawn_blocking(move || hotspot::set_enabled(on)).await.map_err(|e| e.to_string())??;
    http::emit_log(&app, "success", "server", format!("Hotspot {} ({})", info.state, info.ssid.clone().unwrap_or_default()));
    Ok(info)
}

#[tauri::command]
async fn configure_hotspot(ssid: String, passphrase: String) -> Result<HotspotInfo, String> {
    if ssid.trim().is_empty() {
        return Err("The hotspot name cannot be empty.".into());
    }
    if !passphrase.is_empty() && passphrase.chars().count() < 8 {
        return Err("The password must be at least 8 characters.".into());
    }
    tauri::async_runtime::spawn_blocking(move || hotspot::configure(ssid.trim(), &passphrase)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn hotspot_settings_url() -> String {
    hotspot::settings_url().to_string()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ShareResult {
    hotspot: HotspotInfo,
    status: ServerStatus,
    /// True when this call turned the hotspot on (so stop can turn it off again).
    hotspot_started: bool,
}

/// One-click flow: hotspot on (if we can), wait for its adapter, start the server.
#[tauri::command]
async fn auto_share(app: AppHandle, engine: State<'_, Arc<ServerEngine>>, ip: Option<String>) -> Result<ShareResult, String> {
    let mut info = tauri::async_runtime::spawn_blocking(hotspot::info).await.map_err(|e| e.to_string())?;
    let mut hotspot_started = false;
    if info.supported && info.state != "on" {
        http::emit_log(&app, "info", "server", "Turning on Mobile Hotspot…");
        info = tauri::async_runtime::spawn_blocking(|| hotspot::set_enabled(true)).await.map_err(|e| e.to_string())??;
        hotspot_started = true;
        http::emit_log(&app, "success", "server", format!("Hotspot \"{}\" is on", info.ssid.clone().unwrap_or_default()));
    }

    // Prefer the explicit adapter; otherwise wait for a hotspot gateway to appear.
    let bind_ip = match ip {
        Some(ip) => Some(ip),
        None => {
            let mut found = None;
            for _ in 0..40 {
                if let Some(i) = network::list_interfaces().into_iter().find(|i| i.is_hotspot) {
                    found = Some(i.ip);
                    break;
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            if found.is_none() {
                if info.supported {
                    http::emit_log(&app, "warn", "server", "Hotspot adapter not found yet; using the best available adapter");
                }
                found = network::detect_hotspot().map(|i| i.ip);
            }
            found
        }
    };
    engine.start(app.clone(), bind_ip, None).await?;
    // Friendly name: hosts-file entry (UAC on Windows) + verification against the hotspot DNS.
    let status = if cfg!(windows) {
        match engine.setup_name(&app).await {
            Ok(s) => s,
            Err(_) => engine.status(),
        }
    } else {
        engine.verify_name(&app).await
    };
    Ok(ShareResult { hotspot: info, status, hotspot_started })
}

/// Stop the server and optionally turn the hotspot off again.
#[tauri::command]
async fn stop_share(app: AppHandle, engine: State<'_, Arc<ServerEngine>>, turn_off_hotspot: bool) -> Result<ShareResult, String> {
    let status = engine.stop(app.clone()).await;
    let hotspot = if turn_off_hotspot {
        http::emit_log(&app, "info", "server", "Turning off Mobile Hotspot…");
        match tauri::async_runtime::spawn_blocking(|| hotspot::set_enabled(false)).await.map_err(|e| e.to_string())? {
            Ok(i) => i,
            Err(e) => {
                http::emit_log(&app, "warn", "server", format!("Could not turn off the hotspot: {e}"));
                tauri::async_runtime::spawn_blocking(hotspot::info).await.map_err(|e| e.to_string())?
            }
        }
    } else {
        tauri::async_runtime::spawn_blocking(hotspot::info).await.map_err(|e| e.to_string())?
    };
    Ok(ShareResult { hotspot, status, hotspot_started: false })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let dir = app.path().app_config_dir().unwrap_or_else(|_| std::env::temp_dir().join("hotspot-pool"));
            let settings = Settings::load(&dir);
            let pool = Arc::new(PoolState::default());
            // Restore the pool from the previous session; folder sizes are computed in the background.
            let (restored, missing) = pool.load(&dir);
            let handle = app.handle().clone();
            for item in restored.iter().filter(|i| i.is_dir) {
                let pool = pool.clone();
                let id = item.id.clone();
                let path = std::path::PathBuf::from(&item.path);
                let handle = handle.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    let size = pool::dir_size(&path);
                    pool.set_size(&id, size);
                    let _ = handle.emit(EV_POOL, pool.items());
                });
            }
            if !missing.is_empty() {
                let handle = app.handle().clone();
                let n = missing.len();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(1500)).await;
                    http::emit_log(&handle, "warn", "server", format!("{n} pooled item(s) no longer exist and were dropped"));
                });
            }
            let inbox = settings.inbox_dir.clone().map(std::path::PathBuf::from).unwrap_or_else(|| default_inbox_dir(app.handle()));
            let uploads = Arc::new(UploadStore::new(inbox));
            let _ = uploads.ensure_dirs();
            uploads.sweep_parts();
            app.manage(Arc::new(ServerEngine::new(pool, settings, uploads)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_network_interfaces,
            get_hotspot_interface,
            start_server,
            stop_server,
            get_server_status,
            get_pool,
            add_path_to_pool,
            remove_path_from_pool,
            set_pool_password,
            get_hotspot_info,
            set_hotspot,
            configure_hotspot,
            hotspot_settings_url,
            auto_share,
            stop_share,
            get_settings,
            set_settings,
            setup_name,
            check_name,
            list_uploads,
            get_inbox_info,
            approve_upload,
            reject_upload,
            approve_all_uploads,
            reject_all_uploads,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                let engine = window.state::<Arc<ServerEngine>>().inner().clone();
                let app = window.app_handle().clone();
                tauri::async_runtime::block_on(async move {
                    let _ = engine.stop(app).await;
                });
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
