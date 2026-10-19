mod ftp;
mod network;
mod pool;
mod vfs;

use ftp::{FtpEngine, FtpStatus};
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
async fn start_ftp(app: AppHandle, engine: State<'_, Arc<FtpEngine>>, ip: Option<String>, port: Option<u16>) -> Result<FtpStatus, String> {
    engine.start(app, ip, port).await
}

#[tauri::command]
async fn stop_ftp(app: AppHandle, engine: State<'_, Arc<FtpEngine>>) -> Result<FtpStatus, String> {
    Ok(engine.stop(app).await)
}

#[tauri::command]
fn get_ftp_status(engine: State<'_, Arc<FtpEngine>>) -> FtpStatus {
    engine.status()
}

#[tauri::command]
fn get_pool(engine: State<'_, Arc<FtpEngine>>) -> Vec<PoolItem> {
    engine.pool().items()
}

fn broadcast_pool(app: &AppHandle, pool: &PoolState) {
    let _ = app.emit(EV_POOL, pool.items());
}

/// Adds one or more host paths. Folder sizes are computed in the background
/// and pushed via a `pool_changed` event when ready.
#[tauri::command]
fn add_path_to_pool(app: AppHandle, engine: State<'_, Arc<FtpEngine>>, paths: Vec<String>) -> Result<Vec<PoolItem>, String> {
    let pool = engine.pool().clone();
    let mut errors = Vec::new();
    for raw in paths {
        match pool.add(&raw) {
            Ok(item) => {
                ftp::emit_log(&app, "info", "server", format!("Added {} to pool", item.name));
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

#[tauri::command]
fn remove_path_from_pool(app: AppHandle, engine: State<'_, Arc<FtpEngine>>, id: String) -> Vec<PoolItem> {
    let pool = engine.pool();
    if pool.remove(&id) {
        ftp::emit_log(&app, "info", "server", "Removed item from pool");
    }
    broadcast_pool(&app, pool);
    pool.items()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let engine = Arc::new(FtpEngine::new(Arc::new(PoolState::default())));
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(engine)
        .invoke_handler(tauri::generate_handler![
            get_network_interfaces,
            get_hotspot_interface,
            start_ftp,
            stop_ftp,
            get_ftp_status,
            get_pool,
            add_path_to_pool,
            remove_path_from_pool,
        ])
        .on_window_event(|window, event| {
            // Make sure the FTP listener goes away with the window.
            if let tauri::WindowEvent::Destroyed = event {
                let engine = window.state::<Arc<FtpEngine>>().inner().clone();
                let app = window.app_handle().clone();
                tauri::async_runtime::block_on(async move {
                    let _ = engine.stop(app).await;
                });
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
