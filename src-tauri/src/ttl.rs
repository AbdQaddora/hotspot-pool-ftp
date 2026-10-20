//! TTL normalization: raise the TTL on forwarded IPv4 packets so devices on a
//! network that forces the return TTL to 1 (a common MikroTik anti-tethering
//! rule) still get internet through the laptop's hotspot.
//!
//! Windows has no native way to rewrite forwarded-packet TTL, so this uses the
//! WinDivert driver (user-mode packet intercept). The per-packet work lives in
//! [`crate::ttlfix`]; this module is the capture loop, the elevated sidecar
//! process lifecycle, and the detection helper. Only the capture loop needs
//! the driver, so it is built only with the `ttl` feature (which bundles it).

use std::path::PathBuf;
#[cfg(all(windows, feature = "ttl"))]
use std::time::Duration;

/// Raise matched packets to this TTL (a normal Windows default).
pub const TARGET_TTL: u8 = 128;

/// Kernel-side filter: only the already-killed low-TTL inbound IPv4 packets are
/// handed to user mode, so everything else stays in the kernel and untouched.
pub const FILTER: &str = "inbound and ip and ip.TTL <= 4";

fn comms_dir() -> PathBuf {
    std::env::temp_dir().join("hotspot-pool")
}
pub fn stop_path() -> PathBuf {
    comms_dir().join("ttl.stop")
}
pub fn status_path() -> PathBuf {
    comms_dir().join("ttl.status")
}

/// True while a sidecar reports itself running.
pub fn is_running() -> bool {
    std::fs::read_to_string(status_path()).map(|s| s.trim() == "running").unwrap_or(false)
}

/// The sidecar's last status line (e.g. an error), if any.
pub fn status() -> Option<String> {
    std::fs::read_to_string(status_path()).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Ask a running sidecar to stop (it polls this file).
pub fn signal_stop() {
    let _ = std::fs::create_dir_all(comms_dir());
    let _ = std::fs::write(stop_path(), b"stop");
}

/// First TTL value in `ping` reply output ("TTL=1" / "ttl=1"); None if absent.
/// A value of 1 means the network is forcing the TTL down and blocking sharing.
pub fn parse_reply_ttl(output: &str) -> Option<u8> {
    let lo = output.to_ascii_lowercase();
    let idx = lo.find("ttl=")?;
    lo[idx + 4..].chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().ok()
}

/// Ping a public address once and report the reply TTL.
pub fn measure_reply_ttl(target: &str) -> Option<u8> {
    let count_flag = if cfg!(windows) { "-n" } else { "-c" };
    let out = std::process::Command::new("ping").args([count_flag, "1", target]).output().ok()?;
    parse_reply_ttl(&String::from_utf8_lossy(&out.stdout))
}

/// Launch the elevated TTL sidecar (one UAC prompt). Only real with the `ttl` feature.
#[cfg(all(windows, feature = "ttl"))]
pub fn start_sidecar() -> Result<(), String> {
    let _ = std::fs::create_dir_all(comms_dir());
    let _ = std::fs::remove_file(stop_path());
    let _ = std::fs::remove_file(status_path());
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    crate::elevate::run_exe_elevated(&exe.display().to_string(), &["--ttl-fix"])
}

#[cfg(not(all(windows, feature = "ttl")))]
pub fn start_sidecar() -> Result<(), String> {
    Err("This build does not include TTL normalization.".into())
}

/// The `--ttl-fix` elevated process: raise TTL on matched packets until asked
/// to stop or the hotspot goes away.
#[cfg(all(windows, feature = "ttl"))]
pub fn run_sidecar() {
    use windivert::layer::NetworkLayer;
    use windivert::prelude::*;

    let _ = std::fs::create_dir_all(comms_dir());
    // Watcher thread: exit on the stop file or when the hotspot gateway is gone.
    std::thread::spawn(|| {
        let mut missing = 0u32;
        loop {
            std::thread::sleep(Duration::from_millis(1000));
            if stop_path().exists() {
                let _ = std::fs::remove_file(status_path());
                std::process::exit(0);
            }
            let has_gateway = crate::network::list_interfaces().iter().any(|i| i.is_hotspot);
            missing = if has_gateway { 0 } else { missing + 1 };
            if missing >= 8 {
                let _ = std::fs::remove_file(status_path());
                std::process::exit(0);
            }
        }
    });

    let handle: WinDivert<NetworkLayer> = match WinDivert::network(FILTER, 0, WinDivertFlags::new()) {
        Ok(h) => h,
        Err(e) => {
            let _ = std::fs::write(status_path(), format!("error: {e}"));
            return;
        }
    };
    let _ = std::fs::write(status_path(), b"running");

    let mut buf = vec![0u8; 65535];
    loop {
        match handle.recv(Some(&mut buf)) {
            Ok(mut packet) => {
                crate::ttlfix::raise_ttl(packet.data.to_mut(), TARGET_TTL);
                let _ = handle.send(&packet);
            }
            Err(_) => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

#[cfg(not(all(windows, feature = "ttl")))]
pub fn run_sidecar() {
    eprintln!("This build does not include TTL normalization.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_reply_ttl_reads_both_cases() {
        assert_eq!(parse_reply_ttl("Reply from 8.8.8.8: bytes=32 time=70ms TTL=1"), Some(1));
        assert_eq!(parse_reply_ttl("64 bytes from 8.8.8.8: icmp_seq=1 ttl=117 time=10 ms"), Some(117));
        assert_eq!(parse_reply_ttl("Request timed out."), None);
        assert_eq!(parse_reply_ttl("TTL=128 extra"), Some(128));
    }
}
