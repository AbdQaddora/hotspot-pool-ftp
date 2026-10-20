// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args().skip(1).any(|a| a == "--ttl-fix") {
        hotspot_pool_ftp_lib::ttl::run_sidecar();
        return;
    }
    hotspot_pool_ftp_lib::run()
}
