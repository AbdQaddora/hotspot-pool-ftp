//! Friendly name for the share (`<label>.pool`).
//!
//! On a Windows Mobile hotspot the ICS DNS proxy at the gateway resolves on
//! behalf of phones through the laptop's own resolver, so a line in the
//! hosts file makes `abdu.pool` answer for every phone on the hotspot.
//! Writing the hosts file needs one UAC prompt. Resolution is then verified
//! by sending a real DNS query to the gateway.

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
    time::Duration,
};

/// How long one DNS probe waits for an answer.
pub const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);

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

/// Elevated script body that installs the new hosts file and flushes DNS.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn hosts_script(payload: &str, hosts_path: &str) -> String {
    format!(
        "Copy-Item -LiteralPath {} -Destination {} -Force\r\nipconfig /flushdns | Out-Null\r\n",
        crate::elevate::ps_quote(payload),
        crate::elevate::ps_quote(hosts_path)
    )
}

/// Current hosts file as text. Non-UTF-8 bytes are replaced (the file is
/// rewritten line by line, so other lines survive); a failed read is an
/// error, never an empty file that would then replace the real one.
pub fn read_hosts(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Could not read the hosts file {}: {e}", path.display()))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Only a real hotspot gateway address belongs in the hosts file: a
/// link-local (169.254.x.x) address means the adapter had no DHCP lease yet.
fn usable_hotspot_ip(ip: Ipv4Addr) -> Result<(), String> {
    if ip.is_link_local() || ip.is_unspecified() || ip.is_loopback() {
        return Err(format!("{ip} is not a usable hotspot address; the name was not written. Phones can use the IP link once the hotspot has a real address."));
    }
    Ok(())
}

/// Make sure the hosts file maps `name` to `ip`. Returns true when it had to
/// write (after a UAC prompt on Windows).
pub fn ensure_hosts_entry(name: &str, ip: Ipv4Addr) -> Result<bool, String> {
    usable_hotspot_ip(ip)?;
    let path = hosts_path();
    let current = read_hosts(&path)?;
    if hosts_has(&current, name, ip) {
        return Ok(false);
    }
    #[cfg(windows)]
    {
        let new_contents = rewrite_hosts(&current, name, ip);
        let payload = std::env::temp_dir().join("hotspot-pool").join("hosts.new");
        std::fs::create_dir_all(payload.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&payload, new_contents.as_bytes()).map_err(|e| e.to_string())?;
        let script = hosts_script(&payload.display().to_string(), &path.display().to_string());
        crate::elevate::run_elevated("apply-hosts.ps1", &script).map_err(|e| {
            if e.contains("cancelled") {
                "Windows needs permission to add the name to the hosts file. The prompt was cancelled; phones can still use the IP link.".to_string()
            } else {
                format!("Could not update the hosts file: {e}")
            }
        })?;
        let after = read_hosts(&path)?;
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
    resolve_at(SocketAddr::new(server, 53), name, PROBE_TIMEOUT).await
}

/// One DNS query to `server`. None on bind/send/recv error (on Windows a
/// closed port answers with WSAECONNRESET within milliseconds), on
/// timeout, or when the answer carries no A record.
async fn resolve_at(server: SocketAddr, name: &str, timeout: Duration) -> Option<Ipv4Addr> {
    let (id, resp) = exchange_at(server, name, timeout).await?;
    parse_a_record(&resp, id)
}

/// One raw DNS exchange: the transaction id and the response bytes when a
/// reply carrying that id arrived within `timeout`, whatever its RCODE.
async fn exchange_at(server: SocketAddr, name: &str, timeout: Duration) -> Option<(u16, Vec<u8>)> {
    let bind = SocketAddr::new(if server.is_ipv4() { IpAddr::V4(Ipv4Addr::UNSPECIFIED) } else { server.ip() }, 0);
    let sock = tokio::net::UdpSocket::bind(bind).await.ok()?;
    let id: u16 = rand::random();
    sock.send_to(&build_query(id, name), server).await.ok()?;
    let mut buf = [0u8; 512];
    let (n, _) = tokio::time::timeout(timeout, sock.recv_from(&mut buf)).await.ok()?.ok()?;
    if n < 12 || u16::from_be_bytes([buf[0], buf[1]]) != id {
        return None;
    }
    Some((id, buf[..n].to_vec()))
}

/// True when something at `server` answers DNS at all (any RCODE counts):
/// proves the ICS DNS proxy is listening, whether or not it knows `name`.
pub async fn dns_responds(server: IpAddr, name: &str) -> bool {
    responds_at(SocketAddr::new(server, 53), name, PROBE_TIMEOUT).await
}

async fn responds_at(server: SocketAddr, name: &str, timeout: Duration) -> bool {
    exchange_at(server, name, timeout).await.is_some()
}

/// Like [`resolve_via`], but keeps asking: the ICS DNS proxy on the hotspot
/// gateway comes up a little after the adapter does, so an early probe gets
/// an error or nothing. Every failure counts as "not yet" until `attempts`
/// probes, `delay` apart, have all failed.
pub async fn resolve_with_retry(server: IpAddr, name: &str, attempts: u32, delay: Duration) -> Option<Ipv4Addr> {
    resolve_with_retry_at(SocketAddr::new(server, 53), name, attempts, delay, PROBE_TIMEOUT).await
}

async fn resolve_with_retry_at(server: SocketAddr, name: &str, attempts: u32, delay: Duration, timeout: Duration) -> Option<Ipv4Addr> {
    for i in 0..attempts.max(1) {
        if i > 0 {
            tokio::time::sleep(delay).await;
        }
        if let Some(ip) = resolve_at(server, name, timeout).await {
            return Some(ip);
        }
    }
    None
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

    /// Stub DNS server that swallows the first `ignore` queries and answers
    /// every later one with 10.0.0.7. Returns (addr, query counter).
    async fn late_stub(ignore: usize) -> (SocketAddr, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
        let server = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = server.local_addr().unwrap();
        let seen = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen2 = seen.clone();
        tokio::spawn(async move {
            let mut buf = [0u8; 512];
            loop {
                let (n, from) = server.recv_from(&mut buf).await.unwrap();
                let k = seen2.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                if k <= ignore {
                    continue;
                }
                let mut r = buf[..n].to_vec();
                r[2] = 0x81;
                r[3] = 0x80;
                r[7] = 1;
                r.extend_from_slice(&[0xC0, 0x0C, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4, 10, 0, 0, 7]);
                server.send_to(&r, from).await.unwrap();
            }
        });
        (addr, seen)
    }

    #[tokio::test]
    async fn resolve_with_retry_waits_for_late_answer() {
        let (addr, seen) = late_stub(2).await;
        let got = resolve_with_retry_at(addr, "x.pool", 5, Duration::from_millis(20), Duration::from_millis(100)).await;
        assert_eq!(got, Some(Ipv4Addr::new(10, 0, 0, 7)));
        assert_eq!(seen.load(std::sync::atomic::Ordering::SeqCst), 3, "answered on the third query");
    }

    #[tokio::test]
    async fn resolve_with_retry_gives_up_after_budget() {
        let (addr, seen) = late_stub(usize::MAX).await;
        let started = std::time::Instant::now();
        let got = resolve_with_retry_at(addr, "x.pool", 3, Duration::from_millis(20), Duration::from_millis(100)).await;
        assert_eq!(got, None);
        assert_eq!(seen.load(std::sync::atomic::Ordering::SeqCst), 3);
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[tokio::test]
    async fn resolve_with_retry_treats_closed_port_as_not_yet() {
        // Bind then drop so the port is (almost certainly) closed: the kernel
        // answers with ICMP unreachable, which surfaces as a recv error.
        let addr = { tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap().local_addr().unwrap() };
        let got = resolve_with_retry_at(addr, "x.pool", 2, Duration::from_millis(10), Duration::from_millis(100)).await;
        assert_eq!(got, None);
    }

    #[tokio::test]
    async fn dns_responds_counts_any_answer_but_not_silence() {
        // A server that only ever says NXDOMAIN is still a live server.
        let server = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = server.local_addr().unwrap();
        tokio::spawn(async move {
            let mut buf = [0u8; 512];
            let (n, from) = server.recv_from(&mut buf).await.unwrap();
            let mut r = buf[..n].to_vec();
            r[2] = 0x81;
            r[3] = 0x83;
            server.send_to(&r, from).await.unwrap();
        });
        assert!(responds_at(addr, "x.pool", Duration::from_millis(500)).await);
        assert_eq!(resolve_at(addr, "x.pool", Duration::from_millis(100)).await, None, "NXDOMAIN is not an address");
        let closed = { tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap().local_addr().unwrap() };
        assert!(!responds_at(closed, "x.pool", Duration::from_millis(100)).await);
    }

    #[test]
    fn ensure_hosts_entry_refuses_unusable_addresses() {
        for ip in [Ipv4Addr::new(169, 254, 7, 7), Ipv4Addr::UNSPECIFIED, Ipv4Addr::LOCALHOST] {
            let err = ensure_hosts_entry("abdu.pool", ip).unwrap_err();
            assert!(err.contains("not a usable hotspot address"), "{ip}: {err}");
            assert!(err.contains("the name was not written"), "{ip}: {err}");
        }
    }

    #[test]
    fn read_hosts_is_lossy_but_keeps_other_lines() {
        let dir = std::env::temp_dir().join(format!("hpf-hosts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("hosts");
        std::fs::write(&path, b"127.0.0.1 localhost\r\n# caf\xE9 comment\r\n10.1.1.1 old.pool # hotspot-pool\r\n").unwrap();
        let contents = read_hosts(&path).unwrap();
        assert!(contents.contains("127.0.0.1 localhost"));
        assert!(contents.contains("caf\u{FFFD} comment"));
        let new = rewrite_hosts(&contents, "abdu.pool", IP);
        assert!(new.starts_with("127.0.0.1 localhost\r\n# caf\u{FFFD} comment\r\n"));
        assert!(!new.contains("old.pool"));
        assert!(read_hosts(&dir.join("missing")).is_err(), "a failed read must not become an empty file");
        let _ = std::fs::remove_dir_all(dir);
    }
}
