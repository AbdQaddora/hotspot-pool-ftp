//! Friendly name for the share (`<label>.pool`).
//!
//! On a Windows Mobile hotspot the ICS DNS proxy at the gateway resolves on
//! behalf of phones through the laptop's own resolver, so a line in the
//! hosts file makes `abdu.pool` answer for every phone on the hotspot.
//! Writing the hosts file needs one UAC prompt. Resolution is then verified
//! by sending a real DNS query to the gateway.

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    time::Duration,
};

#[cfg_attr(not(windows), allow(dead_code))]
pub const MARKER: &str = "# hotspot-pool";

pub fn hosts_path() -> PathBuf {
    if cfg!(windows) {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
        PathBuf::from(root).join("System32").join("drivers").join("etc").join("hosts")
    } else {
        PathBuf::from("/etc/hosts")
    }
}

/// True when the hosts file maps `name` to `ip` (any line, our marker or not).
pub fn hosts_has(contents: &str, name: &str, ip: Ipv4Addr) -> bool {
    contents.lines().any(|line| {
        let line = line.split('#').next().unwrap_or("").trim();
        let mut parts = line.split_whitespace();
        match parts.next().and_then(|a| a.parse::<Ipv4Addr>().ok()) {
            Some(a) if a == ip => parts.any(|h| h.eq_ignore_ascii_case(name)),
            _ => false,
        }
    })
}

/// New hosts contents: previous lines minus ours, plus the fresh mapping.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn rewrite_hosts(contents: &str, name: &str, ip: Ipv4Addr) -> String {
    let mut out: Vec<&str> = contents.lines().filter(|l| !l.contains(MARKER)).collect();
    while out.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
        out.pop();
    }
    let mut s = out.join("\r\n");
    if !s.is_empty() {
        s.push_str("\r\n");
    }
    s.push_str(&format!("{ip} {name} {MARKER}\r\n"));
    s
}

/// Make sure the hosts file maps `name` to `ip`. Returns true when it had to
/// write (after a UAC prompt on Windows).
pub fn ensure_hosts_entry(name: &str, ip: Ipv4Addr) -> Result<bool, String> {
    let path = hosts_path();
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    if hosts_has(&current, name, ip) {
        return Ok(false);
    }
    #[cfg(windows)]
    {
        let new_contents = rewrite_hosts(&current, name, ip);
        let tmp_dir = std::env::temp_dir().join("hotspot-pool");
        std::fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
        let payload = tmp_dir.join("hosts.new");
        std::fs::write(&payload, new_contents.as_bytes()).map_err(|e| e.to_string())?;
        let script = tmp_dir.join("apply-hosts.ps1");
        std::fs::write(
            &script,
            format!(
                "$ErrorActionPreference = 'Stop'\r\nCopy-Item -LiteralPath '{}' -Destination '{}' -Force\r\nipconfig /flushdns | Out-Null\r\n",
                payload.display().to_string().replace('\'', "''"),
                path.display().to_string().replace('\'', "''")
            ),
        )
        .map_err(|e| e.to_string())?;
        // Outer (non-elevated) PowerShell launches an elevated one and waits for it.
        let arg = format!(
            "Start-Process -FilePath 'powershell.exe' -Verb RunAs -Wait -WindowStyle Hidden -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-File','\"{}\"'",
            script.display()
        );
        let out = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &arg])
            .output()
            .map_err(|e| format!("Could not launch PowerShell: {e}"))?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            if err.contains("canceled") || err.contains("cancelled") || err.contains("1223") {
                return Err("Windows needs permission to add the name to the hosts file. The prompt was cancelled; phones can still use the IP link.".into());
            }
            return Err(format!("Could not update the hosts file: {}", err.trim()));
        }
        let after = std::fs::read_to_string(&path).unwrap_or_default();
        if !hosts_has(&after, name, ip) {
            return Err("The hosts file was not updated (permission denied?). Phones can still use the IP link.".into());
        }
        Ok(true)
    }
    #[cfg(not(windows))]
    {
        let _ = ip;
        Err(format!(
            "Add this line to {} as administrator to enable the name: {ip} {name}",
            path.display()
        ))
    }
}

// ----------------------------------------------------------------- DNS probe

fn build_query(id: u16, name: &str) -> Vec<u8> {
    let mut q = Vec::with_capacity(64);
    q.extend_from_slice(&id.to_be_bytes());
    q.extend_from_slice(&[0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0]);
    for label in name.trim_end_matches('.').split('.') {
        q.push(label.len() as u8);
        q.extend_from_slice(label.as_bytes());
    }
    q.extend_from_slice(&[0, 0, 1, 0, 1]);
    q
}

fn skip_name(buf: &[u8], mut i: usize) -> Option<usize> {
    loop {
        let len = *buf.get(i)? as usize;
        if len == 0 {
            return Some(i + 1);
        }
        if len & 0xC0 == 0xC0 {
            return Some(i + 2);
        }
        i += 1 + len;
    }
}

/// First A record in a DNS response, if any.
pub fn parse_a_record(resp: &[u8], id: u16) -> Option<Ipv4Addr> {
    if resp.len() < 12 || u16::from_be_bytes([resp[0], resp[1]]) != id {
        return None;
    }
    if resp[3] & 0x0F != 0 {
        return None; // RCODE != NOERROR
    }
    let qd = u16::from_be_bytes([resp[4], resp[5]]) as usize;
    let an = u16::from_be_bytes([resp[6], resp[7]]) as usize;
    let mut i = 12;
    for _ in 0..qd {
        i = skip_name(resp, i)? + 4;
    }
    for _ in 0..an {
        i = skip_name(resp, i)?;
        let rtype = u16::from_be_bytes([*resp.get(i)?, *resp.get(i + 1)?]);
        let rdlen = u16::from_be_bytes([*resp.get(i + 8)?, *resp.get(i + 9)?]) as usize;
        i += 10;
        if rtype == 1 && rdlen == 4 {
            return Some(Ipv4Addr::new(*resp.get(i)?, *resp.get(i + 1)?, *resp.get(i + 2)?, *resp.get(i + 3)?));
        }
        i += rdlen;
    }
    None
}

/// Ask `server` (UDP/53) what `name` resolves to; None on timeout/NXDOMAIN.
pub async fn resolve_via(server: IpAddr, name: &str) -> Option<Ipv4Addr> {
    let sock = tokio::net::UdpSocket::bind(SocketAddr::new(if server.is_ipv4() { IpAddr::V4(Ipv4Addr::UNSPECIFIED) } else { server }, 0)).await.ok()?;
    let id: u16 = rand::random();
    sock.send_to(&build_query(id, name), SocketAddr::new(server, 53)).await.ok()?;
    let mut buf = [0u8; 512];
    let (n, _) = tokio::time::timeout(Duration::from_millis(1500), sock.recv_from(&mut buf)).await.ok()?.ok()?;
    parse_a_record(&buf[..n], id)
}

/// The laptop's computer name, which the ICS DNS proxy also resolves for phones.
pub fn hostname() -> Option<String> {
    std::env::var("COMPUTERNAME")
        .ok()
        .or_else(|| std::env::var("HOSTNAME").ok())
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
        .map(|h| h.trim().to_ascii_lowercase())
        .filter(|h| !h.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    const IP: Ipv4Addr = Ipv4Addr::new(192, 168, 137, 1);

    #[test]
    fn hosts_detection_and_rewrite() {
        let base = "# Copyright\r\n127.0.0.1 localhost\r\n192.168.137.1 old.pool # hotspot-pool\r\n";
        assert!(hosts_has(base, "old.pool", IP));
        assert!(hosts_has(base, "OLD.POOL", IP));
        assert!(!hosts_has(base, "abdu.pool", IP));
        let new = rewrite_hosts(base, "abdu.pool", IP);
        assert!(!new.contains("old.pool"));
        assert!(new.ends_with("192.168.137.1 abdu.pool # hotspot-pool\r\n"));
        assert!(new.contains("127.0.0.1 localhost"));
        assert!(hosts_has(&new, "abdu.pool", IP));
    }

    #[test]
    fn dns_roundtrip_parsing() {
        let q = build_query(0x1234, "abdu.pool");
        assert_eq!(&q[12..], &[4, b'a', b'b', b'd', b'u', 4, b'p', b'o', b'o', b'l', 0, 0, 1, 0, 1]);
        // Build a response: header + question + one compressed answer.
        let mut r = q.clone();
        r[2] = 0x81;
        r[3] = 0x80;
        r[7] = 1;
        r.extend_from_slice(&[0xC0, 0x0C, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 192, 168, 137, 1]);
        assert_eq!(parse_a_record(&r, 0x1234), Some(IP));
        assert_eq!(parse_a_record(&r, 0x9999), None);
        let mut nx = r.clone();
        nx[3] = 0x83;
        assert_eq!(parse_a_record(&nx, 0x1234), None);
    }

    #[tokio::test]
    async fn resolve_against_local_stub() {
        let server = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = server.local_addr().unwrap();
        tokio::spawn(async move {
            let mut buf = [0u8; 512];
            let (n, from) = server.recv_from(&mut buf).await.unwrap();
            let mut r = buf[..n].to_vec();
            r[2] = 0x81;
            r[3] = 0x80;
            r[7] = 1;
            r.extend_from_slice(&[0xC0, 0x0C, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 10, 0, 0, 7]);
            server.send_to(&r, from).await.unwrap();
        });
        // resolve_via always targets port 53; emulate by calling the internals directly.
        let sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let id = 7u16;
        sock.send_to(&build_query(id, "x.pool"), addr).await.unwrap();
        let mut buf = [0u8; 512];
        let (n, _) = sock.recv_from(&mut buf).await.unwrap();
        assert_eq!(parse_a_record(&buf[..n], id), Some(Ipv4Addr::new(10, 0, 0, 7)));
    }
}
