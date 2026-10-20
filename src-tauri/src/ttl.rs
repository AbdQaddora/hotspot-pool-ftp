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

/// True when this build can actually run TTL normalization (the driver is bundled).
pub fn available() -> bool {
    cfg!(all(windows, feature = "ttl"))
}

/// Raise matched packets to this TTL (a normal Windows default).
pub const TARGET_TTL: u8 = 128;

/// Kernel-side filter: only the already-killed low-TTL inbound IPv4 packets are
/// handed to user mode, so everything else stays in the kernel and untouched.
pub const FILTER: &str = "inbound and ip and ip.TTL <= 4";

/// Shared files between the app and the elevated sidecar. The app passes this
/// directory to the sidecar (`--comms`) because an elevated process may not
/// share the app's `%TEMP%` (standard user + separate admin account).
fn comms_dir() -> PathBuf {
    std::env::temp_dir().join("hotspot-pool")
}
fn stop_path_in(base: &std::path::Path) -> PathBuf {
    base.join("ttl.stop")
}
fn status_path_in(base: &std::path::Path) -> PathBuf {
    base.join("ttl.status")
}
pub fn stop_path() -> PathBuf {
    stop_path_in(&comms_dir())
}
pub fn status_path() -> PathBuf {
    status_path_in(&comms_dir())
}

/// True while a sidecar reports itself running and no stop has been signalled.
pub fn is_running() -> bool {
    if stop_path().exists() {
        return false;
    }
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

/// The `--comms <dir>` value from an argument iterator, if present.
fn parse_comms_arg(mut args: impl Iterator<Item = String>) -> Option<String> {
    while let Some(a) = args.next() {
        if a == "--comms" {
            return args.next();
        }
    }
    None
}

/// Ping a public address once and report the reply TTL.
pub fn measure_reply_ttl(target: &str) -> Option<u8> {
    let count_flag = if cfg!(windows) { "-n" } else { "-c" };
    let mut cmd = std::process::Command::new("ping");
    cmd.args([count_flag, "1", target]);
    // No console window flash: the app is a windowed subsystem.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().ok()?;
    parse_reply_ttl(&String::from_utf8_lossy(&out.stdout))
}

/// Launch the elevated TTL sidecar (one UAC prompt). Only real with the `ttl` feature.
#[cfg(all(windows, feature = "ttl"))]
pub fn start_sidecar() -> Result<(), String> {
    let dir = comms_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::remove_file(stop_path());
    let _ = std::fs::remove_file(status_path());
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    crate::elevate::run_exe_elevated(&exe.display().to_string(), &["--ttl-fix", "--comms", &dir.display().to_string()])
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

    let base = parse_comms_arg(std::env::args()).map(PathBuf::from).unwrap_or_else(comms_dir);
    let _ = std::fs::create_dir_all(&base);
    let status_file = status_path_in(&base);
    let stop_file = stop_path_in(&base);

    // Watcher thread: exit on the stop file or when the hotspot gateway (a
    // score>=90 interface, i.e. a known gateway IP) has been gone for a while.
    {
        let (status_file, stop_file) = (status_file.clone(), stop_file.clone());
        std::thread::spawn(move || {
            let mut missing = 0u32;
            loop {
                std::thread::sleep(Duration::from_millis(1000));
                if stop_file.exists() {
                    let _ = std::fs::remove_file(&status_file);
                    std::process::exit(0);
                }
                let has_gateway = crate::network::list_interfaces().iter().any(|i| i.score >= 90);
                missing = if has_gateway { 0 } else { missing + 1 };
                if missing >= 8 {
                    let _ = std::fs::remove_file(&status_file);
                    std::process::exit(0);
                }
            }
        });
    }

    let handle: WinDivert<NetworkLayer> = match WinDivert::network(FILTER, 0, WinDivertFlags::new()) {
        Ok(h) => h,
        Err(e) => {
            let _ = std::fs::write(&status_file, format!("error: {e}"));
            return;
        }
    };
    let _ = std::fs::write(&status_file, b"running");

    let mut buf = vec![0u8; 65535];
    let mut errors = 0u32;
    loop {
        match handle.recv(Some(&mut buf)) {
            Ok(mut packet) => {
                errors = 0;
                crate::ttlfix::raise_ttl(packet.data.to_mut(), TARGET_TTL);
                let _ = handle.send(&packet);
            }
            Err(e) => {
                errors += 1;
                // A handful of transient errors is fine; a storm means the handle is dead.
                if errors >= 20 {
                    let _ = std::fs::write(&status_file, format!("error: capture stopped ({e})"));
                    std::process::exit(1);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
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
    fn parse_comms_arg_reads_the_value() {
        assert_eq!(parse_comms_arg(["--ttl-fix".into(), "--comms".into(), "C:/x/y".into()].into_iter()), Some("C:/x/y".into()));
        assert_eq!(parse_comms_arg(["--ttl-fix".into()].into_iter()), None);
        assert_eq!(parse_comms_arg(["--comms".into()].into_iter()), None);
    }

    #[test]
    fn parse_reply_ttl_reads_both_cases() {
        assert_eq!(parse_reply_ttl("Reply from 8.8.8.8: bytes=32 time=70ms TTL=1"), Some(1));
        assert_eq!(parse_reply_ttl("64 bytes from 8.8.8.8: icmp_seq=1 ttl=117 time=10 ms"), Some(117));
        assert_eq!(parse_reply_ttl("Request timed out."), None);
        assert_eq!(parse_reply_ttl("TTL=128 extra"), Some(128));
    }
}
