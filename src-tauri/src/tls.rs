//! Self-signed certificate for the HTTPS listener, persisted so phones can
//! remember the "proceed anyway" exception between sessions.

use std::{net::Ipv4Addr, path::Path};

pub struct TlsMaterial {
    pub cert_pem: String,
    pub key_pem: String,
}

fn install_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// Loads `cert.pem`/`key.pem` from `dir` if they were issued for the same
/// names, otherwise generates a fresh 3-year certificate.
pub fn load_or_create(dir: &Path, names: &[String], ip: Ipv4Addr) -> Result<TlsMaterial, String> {
    install_provider();
    let mut sans: Vec<String> = names.iter().map(|n| n.to_ascii_lowercase()).collect();
    sans.push(ip.to_string());
    sans.sort();
    sans.dedup();
    let stamp = sans.join(",");

    let cert_path = dir.join("cert.pem");
    let key_path = dir.join("key.pem");
    let stamp_path = dir.join("cert.sans");
    if let (Ok(c), Ok(k), Ok(s)) = (std::fs::read_to_string(&cert_path), std::fs::read_to_string(&key_path), std::fs::read_to_string(&stamp_path)) {
        if s.trim() == stamp {
            return Ok(TlsMaterial { cert_pem: c, key_pem: k });
        }
    }

    let mut params = rcgen::CertificateParams::new(sans.clone()).map_err(|e| e.to_string())?;
    let mut dn = rcgen::DistinguishedName::new();
    dn.push(rcgen::DnType::CommonName, "Hotspot Pool");
    dn.push(rcgen::DnType::OrganizationName, "Hotspot Pool FTP");
    params.distinguished_name = dn;
    let now = time::OffsetDateTime::now_utc();
    params.not_before = now - time::Duration::days(1);
    params.not_after = now + time::Duration::days(3 * 365);
    let key = rcgen::KeyPair::generate().map_err(|e| e.to_string())?;
    let cert = params.self_signed(&key).map_err(|e| e.to_string())?;
    let material = TlsMaterial { cert_pem: cert.pem(), key_pem: key.serialize_pem() };

    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    std::fs::write(&cert_path, &material.cert_pem).map_err(|e| e.to_string())?;
    std::fs::write(&key_path, &material.key_pem).map_err(|e| e.to_string())?;
    std::fs::write(&stamp_path, stamp).map_err(|e| e.to_string())?;
    Ok(material)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_and_reuses() {
        let dir = std::env::temp_dir().join(format!("hpf-tls-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let a = load_or_create(&dir, &["abdu.pool".into()], Ipv4Addr::new(192, 168, 137, 1)).unwrap();
        assert!(a.cert_pem.contains("BEGIN CERTIFICATE"));
        let b = load_or_create(&dir, &["abdu.pool".into()], Ipv4Addr::new(192, 168, 137, 1)).unwrap();
        assert_eq!(a.cert_pem, b.cert_pem, "same names reuse the stored cert");
        let c = load_or_create(&dir, &["other.pool".into()], Ipv4Addr::new(192, 168, 137, 1)).unwrap();
        assert_ne!(a.cert_pem, c.cert_pem, "new names regenerate");
        let _ = std::fs::remove_dir_all(dir);
    }
}
