mod elevate;
mod hotspot;
mod http;
mod ics;
mod names;
mod network;
mod pool;
mod settings;
mod ttlfix;
mod uploads;
mod vpath;
mod wfd;

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
    let upstream_adapter = match settings.upstream_adapter.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(id) => Some(ics::norm_guid(id).ok_or_else(|| "Pick a connection from the list.".to_string())?),
        None => None,
    };
    let hotspot_engine = Settings::validate_engine(&settings.hotspot_engine)?;
    let next = Settings { name, suffix, upload_pin, max_upload_mb, inbox_quota_mb, inbox_dir: inbox_dir.clone(), upstream_adapter, hotspot_engine, ..settings };
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

/// Our own access point + Windows Internet Connection Sharing, the way
/// MyPublicWiFi does it: Mobile Hotspot off, Wi-Fi Direct AP on with the same
/// SSID/password, ICS from the upstream to the AP's adapter, then wait for
/// Windows to give the adapter 192.168.137.1.
async fn wifidirect_on(app: &AppHandle, preferred: Option<String>, share_internet: bool) -> Result<HotspotInfo, wfd::StartFail> {
    use wfd::StartFail::{AfterAp, ApStart};
    fn join<E: std::fmt::Display>(e: E) -> wfd::StartFail {
        wfd::StartFail::ApStart(e.to_string())
    }
    let base = tauri::async_runtime::spawn_blocking(hotspot::mobile_info).await.map_err(join)?;
    let mut mobile_was_on = false;
    if base.state == "on" || base.state == "transition" {
        http::emit_log(app, "info", "server", "Turning off Mobile Hotspot (it cannot run together with the access point)…");
        tauri::async_runtime::spawn_blocking(|| hotspot::set_enabled(false, None)).await.map_err(join)?.map_err(ApStart)?;
        mobile_was_on = true;
    }
    let ssid = base.ssid.clone().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| "Hotspot Pool".into());
    let passphrase = match base.passphrase.clone().filter(|p| p.chars().count() >= 8) {
        Some(p) => p,
        None => {
            let p = wfd::random_passphrase();
            http::emit_log(app, "warn", "server", format!("Windows has no saved hotspot password; using {p} for this session (set one in Settings)"));
            p
        }
    };

    // Snapshot the Wi-Fi Direct adapters first: the group owner's one is the one that comes Up.
    let before = tauri::async_runtime::spawn_blocking(wfd::wfd_adapters).await.map_err(join)?;
    http::emit_log(app, "info", "server", "Starting the access point…");
    let mut attempt = 0;
    loop {
        attempt += 1;
        let (s, p) = (ssid.clone(), passphrase.clone());
        match tauri::async_runtime::spawn_blocking(move || wfd::start(&s, &p)).await.map_err(join)? {
            Ok(_) => break,
            // Right after Mobile Hotspot stops, the radio can still be busy for a moment.
            Err(e) if wfd::is_resource_in_use(&e) && mobile_was_on && attempt < 3 => {
                http::emit_log(app, "info", "server", "Wi-Fi adapter still busy; retrying in 2 s…");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Err(e) => return Err(ApStart(e)),
        }
    }
    http::emit_log(app, "success", "server", format!("Access point \"{ssid}\" is on"));

    if !share_internet {
        // Files only: no ICS, so no admin prompt. Windows gives the group-owner adapter its own address.
        http::emit_log(app, "info", "server", "Internet sharing is off; sharing files only (no internet for devices).");
        return tauri::async_runtime::spawn_blocking(hotspot::info).await.map_err(|e| ApStart(e.to_string()));
    }

    // Windows brings the group-owner adapter up a moment after the AP starts.
    let mut adapter = None;
    let mut last = Vec::new();
    for _ in 0..10 {
        let after = tauri::async_runtime::spawn_blocking(wfd::wfd_adapters).await.map_err(join)?;
        if let Some(a) = wfd::pick_go_adapter(&before, &after) {
            adapter = Some(a);
            break;
        }
        last = after;
        tokio::time::sleep(Duration::from_millis(800)).await;
    }
    let Some(adapter) = adapter else {
        return Err(ApStart(format!("Windows did not bring up the access point's network adapter (Wi-Fi Direct adapters: {}).", wfd::describe_adapters(&last))));
    };
    let adapter_id = adapter.guid.clone();
    http::emit_log(app, "info", "server", format!("Access point adapter: {}", adapter.name));

    let sel = preferred.clone();
    let snap = tauri::async_runtime::spawn_blocking(move || ics::snapshot(sel.as_deref(), true)).await.map_err(|e| AfterAp(e.to_string()))?;
    let Some(public) = snap.effective_upstream.clone() else { return Err(AfterAp("Connect the laptop to the internet first.".into())) };
    let upstream_name = snap.upstreams.iter().find(|u| u.id == public).map(|u| u.name.clone()).unwrap_or_else(|| "the internet connection".into());
    let already = snap.sharing.available && snap.sharing.public_id.as_deref() == Some(public.as_str()) && snap.sharing.private_id.as_deref() == Some(adapter_id.as_str());
    let apply = |app: AppHandle, public: String, private: String, upstream_name: String| async move {
        http::emit_log(&app, "info", "server", format!("Sharing {upstream_name} with the access point (Windows will ask for permission)…"));
        tauri::async_runtime::spawn_blocking(move || ics::apply_sharing(&public, &private)).await.map_err(|e| AfterAp(e.to_string()))?.map_err(AfterAp)
    };
    let wait = |adapter_id: String, secs: u64| async move { tauri::async_runtime::spawn_blocking(move || ics::wait_for_gateway(&adapter_id, Duration::from_secs(secs))).await.map_err(|e| AfterAp(e.to_string())) };

    http::emit_log(app, "info", "server", "Waiting for Windows to give the access point its address…");
    let mut found = if already {
        // Windows remembered the pairing; it usually re-addresses the adapter by itself…
        wait(adapter_id.clone(), 8).await?
    } else {
        apply(app.clone(), public.clone(), adapter_id.clone(), upstream_name.clone()).await?;
        wait(adapter_id.clone(), 20).await?
    };
    if found.is_none() && already {
        // …but not always ("stuck on 169.254.x.x"): re-apply once, which disables and re-enables sharing.
        apply(app.clone(), public.clone(), adapter_id.clone(), upstream_name.clone()).await?;
        found = wait(adapter_id.clone(), 20).await?;
    }
    if found.is_none() {
        return Err(AfterAp("Windows did not give the access point its address (192.168.137.1). Open Share Wi-Fi and click Fix sharing, then try again.".into()));
    }
    http::emit_log(app, "success", "server", format!("Access point ready at 192.168.137.1; devices get the laptop's internet through {upstream_name}"));
    tauri::async_runtime::spawn_blocking(hotspot::info).await.map_err(|e| AfterAp(e.to_string()))
}

/// Turn the hotspot on with the configured engine; the access point engine
/// falls back to Windows Mobile Hotspot when it cannot start.
async fn hotspot_on(app: &AppHandle, engine: &Arc<ServerEngine>) -> Result<HotspotInfo, String> {
    let settings = engine.settings();
    let preferred = settings.upstream_adapter.clone();
    let share_internet = settings.share_internet;
    if settings.hotspot_engine == settings::ENGINE_WIFIDIRECT && cfg!(windows) {
        match wifidirect_on(app, preferred.clone(), share_internet).await {
            Ok(info) => {
                wfd::set_last_failure(None);
                return Ok(info);
            }
            Err(e) => {
                tauri::async_runtime::spawn_blocking(wfd::stop).await.map_err(|e| e.to_string())?;
                wfd::set_last_failure(Some(e.message().to_string()));
                if !e.allows_mobile_fallback() {
                    // Sharing has been touched: Mobile Hotspot on top of classic ICS is the conflict we are avoiding.
                    return Err(e.message().to_string());
                }
                http::emit_log(app, "warn", "server", format!("{} — using Windows Mobile Hotspot instead.", e.message()));
            }
        }
    }
    let shared = tauri::async_runtime::spawn_blocking(|| ics::sharing_state_cached(false)).await.map_err(|e| e.to_string())?;
    if let Some(public) = shared.public_id.as_deref() {
        let name = shared.connections.iter().find(|c| c.id == public).map(|c| c.name.clone()).unwrap_or_else(|| "a connection".into());
        http::emit_log(app, "warn", "server", format!("Internet Connection Sharing is still enabled on {name}; Windows Mobile Hotspot may refuse to share internet while it is. Turn sharing off in Network Connections if phones get no internet."));
    }
    if !share_internet {
        http::emit_log(app, "warn", "server", "Windows Mobile Hotspot always shares internet; to share files only, use the access point engine in Settings.");
    }
    http::emit_log(app, "info", "server", "Turning on Mobile Hotspot…");
    let info = tauri::async_runtime::spawn_blocking(move || hotspot::set_enabled(true, preferred.as_deref())).await.map_err(|e| e.to_string())??;
    http::emit_log(app, "success", "server", format!("Hotspot \"{}\" is on", info.ssid.clone().unwrap_or_default()));
    Ok(info)
}

/// Turn off whichever hotspot is running.
async fn hotspot_off(app: &AppHandle) -> Result<HotspotInfo, String> {
    if wfd::info().running {
        http::emit_log(app, "info", "server", "Stopping the access point…");
        tauri::async_runtime::spawn_blocking(wfd::stop).await.map_err(|e| e.to_string())?;
        return tauri::async_runtime::spawn_blocking(hotspot::info).await.map_err(|e| e.to_string());
    }
    http::emit_log(app, "info", "server", "Turning off Mobile Hotspot…");
    tauri::async_runtime::spawn_blocking(|| hotspot::set_enabled(false, None)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn set_hotspot(app: AppHandle, engine: State<'_, Arc<ServerEngine>>, on: bool) -> Result<HotspotInfo, String> {
    if on {
        hotspot_on(&app, engine.inner()).await
    } else {
        hotspot_off(&app).await
    }
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
async fn get_wifi_share_status(engine: State<'_, Arc<ServerEngine>>, fresh: bool) -> Result<ics::WifiShareStatus, String> {
    let settings = engine.settings();
    tauri::async_runtime::spawn_blocking(move || ics::snapshot_with_engine(settings.upstream_adapter.as_deref(), fresh, &settings.hotspot_engine)).await.map_err(|e| e.to_string())
}

/// Plain-text support report (no passwords): status snapshot + Wi-Fi Direct adapters.
#[tauri::command]
async fn get_diagnostics(engine: State<'_, Arc<ServerEngine>>) -> Result<String, String> {
    let settings = engine.settings();
    tauri::async_runtime::spawn_blocking(move || {
        let st = ics::snapshot_with_engine(settings.upstream_adapter.as_deref(), true, &settings.hotspot_engine);
        let adapters = wfd::wfd_adapters();
        ics::format_diagnostics(env!("CARGO_PKG_VERSION"), &st, &adapters)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Wire Windows Internet Connection Sharing from the chosen upstream to the
/// hotspot adapter (one UAC prompt), then return a fresh snapshot.
#[tauri::command]
async fn apply_internet_sharing(app: AppHandle, engine: State<'_, Arc<ServerEngine>>) -> Result<ics::WifiShareStatus, String> {
    let settings = engine.settings();
    let selected = settings.upstream_adapter.clone();
    let eng = settings.hotspot_engine.clone();
    let (sel, en) = (selected.clone(), eng.clone());
    let before = tauri::async_runtime::spawn_blocking(move || ics::snapshot_with_engine(sel.as_deref(), true, &en)).await.map_err(|e| e.to_string())?;
    let Some(public) = before.effective_upstream.clone() else { return Err("Connect the laptop to the internet first.".into()) };
    let Some(private) = before.hotspot_adapter.as_ref().and_then(|a| a.adapter_id.clone()) else {
        return Err("Turn on the hotspot first; Windows creates its adapter then.".into());
    };
    let upstream_name = before.upstreams.iter().find(|u| u.id == public).map(|u| u.name.clone()).unwrap_or_default();
    http::emit_log(&app, "info", "server", format!("Sharing {upstream_name} with the hotspot (Windows will ask for permission)…"));
    tauri::async_runtime::spawn_blocking(move || ics::apply_sharing(&public, &private)).await.map_err(|e| e.to_string())??;
    let after = tauri::async_runtime::spawn_blocking(move || ics::snapshot_with_engine(selected.as_deref(), true, &eng)).await.map_err(|e| e.to_string())?;
    if after.health == "sharing" {
        http::emit_log(&app, "success", "server", format!("Devices on the hotspot now use {upstream_name} for internet"));
        Ok(after)
    } else {
        let why = after.sharing.error.clone().unwrap_or_else(|| after.health.replace('_', " "));
        http::emit_log(&app, "warn", "server", format!("Internet Connection Sharing was applied but Windows still reports: {why}"));
        Err(format!("Sharing was applied, but Windows still reports: {why}. Try turning the hotspot off and on, then Fix sharing again."))
    }
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
    // A Windows Mobile Hotspot that is already on must not be inherited by the access point engine: it would not share internet.
    let settings = engine.settings();
    let wrong_engine = cfg!(windows) && settings.hotspot_engine == settings::ENGINE_WIFIDIRECT && info.state == "on" && !wfd::info().running;
    if info.supported && (info.state != "on" || wrong_engine) {
        if wrong_engine {
            http::emit_log(&app, "info", "server", "Switching from Windows Mobile Hotspot to the access point…");
        }
        info = hotspot_on(&app, engine.inner()).await?;
        hotspot_started = true;
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
        match hotspot_off(&app).await {
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
            get_wifi_share_status,
            get_diagnostics,
            apply_internet_sharing,
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
                // Unlike Mobile Hotspot, our access point lives in this process.
                wfd::stop();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
