//! OS hotspot control.
//!
//! Windows: the WinRT tethering API (`NetworkOperatorTetheringManager`), the
//! same thing the Settings "Mobile hotspot" switch uses. No admin needed, but
//! Windows only starts a hotspot when there is a connection to share.
//! Other platforms: reported as unsupported; the UI points to the OS settings.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HotspotInfo {
    /// True when this OS can be controlled from the app.
    pub supported: bool,
    /// "on" | "off" | "transition" | "unknown" | "unsupported"
    pub state: &'static str,
    pub ssid: Option<String>,
    pub passphrase: Option<String>,
    pub client_count: Option<u32>,
    /// Wi-Fi join payload for a QR code, when credentials are known.
    pub wifi_qr: Option<String>,
    /// Human readable reason when the API is unavailable.
    pub error: Option<String>,
}

impl HotspotInfo {
    pub fn unsupported(reason: impl Into<String>) -> Self {
        HotspotInfo { supported: false, state: "unsupported", ssid: None, passphrase: None, client_count: None, wifi_qr: None, error: Some(reason.into()) }
    }
}

/// A device joined to the hotspot.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HotspotClient {
    pub mac: String,
    /// DHCP/mDNS name when Windows knows one (it usually does not).
    pub name: Option<String>,
    pub ip: Option<String>,
}

#[cfg_attr(not(windows), allow(dead_code))]
/// Windows lists a client's HostNames by type (0 DomainName, 1 Ipv4, 2 Ipv6,
/// 3 Bluetooth); only a DomainName is a name, the Ipv4 one is its address.
pub fn split_hostnames(entries: &[(i32, String)]) -> (Option<String>, Option<String>) {
    let pick = |t: i32| entries.iter().find(|(k, v)| *k == t && !v.is_empty()).map(|(_, v)| v.clone());
    (pick(0), pick(1))
}

/// Wi-Fi join string for QR codes (ZXing format understood by iOS/Android cameras).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn wifi_qr(ssid: &str, passphrase: Option<&str>) -> String {
    let esc = |s: &str| s.replace('\\', "\\\\").replace(';', "\\;").replace(',', "\\,").replace(':', "\\:").replace('"', "\\\"");
    match passphrase {
        Some(p) if !p.is_empty() => format!("WIFI:T:WPA;S:{};P:{};;", esc(ssid), esc(p)),
        _ => format!("WIFI:T:nopass;S:{};;", esc(ssid)),
    }
}

#[cfg(windows)]
mod imp {
    use super::HotspotInfo;
    use windows::Networking::Connectivity::{ConnectionProfile, NetworkConnectivityLevel, NetworkInformation};
    use windows::Networking::NetworkOperators::{
        NetworkOperatorTetheringAccessPointConfiguration, NetworkOperatorTetheringManager, TetheringOperationStatus, TetheringOperationalState,
    };

    pub(super) fn init_apartment() {
        use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
        // Safe to call repeatedly; errors (already initialised in another mode) are ignored.
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
    }

    /// The connection the hotspot shares: the preferred adapter when it is
    /// connected, otherwise what Windows calls the internet connection.
    fn pick_profile(preferred: Option<&str>) -> Result<ConnectionProfile, String> {
        if let Some(want) = preferred {
            if let Ok(profiles) = NetworkInformation::GetConnectionProfiles() {
                for p in profiles {
                    let id = p.NetworkAdapter().ok().and_then(|a| a.NetworkAdapterId().ok()).map(|g| crate::ics::guid_string(&g));
                    if id.as_deref() == Some(want) {
                        return Ok(p);
                    }
                }
            }
        }
        if let Ok(p) = NetworkInformation::GetInternetConnectionProfile() {
            return Ok(p);
        }
        // No internet profile: try any connected profile (local networks also work).
        let profiles = NetworkInformation::GetConnectionProfiles().map_err(|e| e.message().to_string())?;
        let mut fallback = None;
        for p in profiles {
            match p.GetNetworkConnectivityLevel() {
                Ok(level) if level != NetworkConnectivityLevel::None => return Ok(p),
                Ok(_) => fallback = fallback.or(Some(p)),
                Err(_) => {}
            }
        }
        fallback.ok_or_else(|| "Windows needs a network connection to share. Connect the laptop to Wi-Fi or Ethernet, then try again.".to_string())
    }

    fn manager() -> Result<NetworkOperatorTetheringManager, String> {
        manager_for(None)
    }

    fn manager_for(preferred: Option<&str>) -> Result<NetworkOperatorTetheringManager, String> {
        init_apartment();
        let profile = pick_profile(preferred)?;
        NetworkOperatorTetheringManager::CreateFromConnectionProfile(&profile)
            .map_err(|e| format!("Mobile hotspot is not available on this adapter ({})", e.message()))
    }

    fn describe(mgr: &NetworkOperatorTetheringManager) -> HotspotInfo {
        let state = match mgr.TetheringOperationalState() {
            Ok(TetheringOperationalState::On) => "on",
            Ok(TetheringOperationalState::Off) => "off",
            Ok(TetheringOperationalState::InTransition) => "transition",
            _ => "unknown",
        };
        let (ssid, passphrase) = match mgr.GetCurrentAccessPointConfiguration() {
            Ok(cfg) => (cfg.Ssid().ok().map(|s| s.to_string()), cfg.Passphrase().ok().map(|s| s.to_string())),
            Err(_) => (None, None),
        };
        let wifi_qr = ssid.as_deref().map(|s| super::wifi_qr(s, passphrase.as_deref()));
        HotspotInfo { supported: true, state, ssid, passphrase, client_count: mgr.ClientCount().ok(), wifi_qr, error: None }
    }

    pub fn info() -> HotspotInfo {
        match manager() {
            Ok(m) => describe(&m),
            Err(e) => HotspotInfo::unsupported(e),
        }
    }

    pub fn clients() -> Vec<super::HotspotClient> {
        let Ok(mgr) = manager() else { return vec![] };
        let Ok(list) = mgr.GetTetheringClients() else { return vec![] };
        list.into_iter()
            .map(|c| {
                let mac = c.MacAddress().map(|s| s.to_string()).unwrap_or_default();
                let entries: Vec<(i32, String)> = c
                    .HostNames()
                    .map(|h| h.into_iter().filter_map(|h| Some((h.Type().ok()?.0, h.DisplayName().ok()?.to_string()))).collect())
                    .unwrap_or_default();
                let (name, ip) = super::split_hostnames(&entries);
                super::HotspotClient { mac, name, ip }
            })
            .collect()
    }

    pub fn set_enabled(on: bool, preferred: Option<&str>) -> Result<HotspotInfo, String> {
        let mgr = manager_for(preferred)?;
        let current = mgr.TetheringOperationalState().map_err(|e| e.message().to_string())?;
        if (on && current == TetheringOperationalState::On) || (!on && current == TetheringOperationalState::Off) {
            return Ok(describe(&mgr));
        }
        let op = if on { mgr.StartTetheringAsync() } else { mgr.StopTetheringAsync() }.map_err(|e| e.message().to_string())?;
        let result = op.join().map_err(|e| e.message().to_string())?;
        let status = result.Status().map_err(|e| e.message().to_string())?;
        if status != TetheringOperationStatus::Success && status != TetheringOperationStatus::AlreadyOn {
            let extra = result.AdditionalErrorMessage().map(|s| s.to_string()).unwrap_or_default();
            let why = match status {
                TetheringOperationStatus::WiFiDeviceOff => "Wi-Fi is turned off. Turn Wi-Fi on first.".to_string(),
                TetheringOperationStatus::MobileBroadbandDeviceOff => "The mobile broadband device is off.".to_string(),
                TetheringOperationStatus::EntitlementCheckFailure | TetheringOperationStatus::EntitlementCheckTimeout => "Windows does not allow the hotspot on this connection.".to_string(),
                TetheringOperationStatus::RadioRestriction => "The Wi-Fi radio is restricted (airplane mode or policy).".to_string(),
                TetheringOperationStatus::OperationInProgress => "Another hotspot operation is still in progress. Try again in a moment.".to_string(),
                TetheringOperationStatus::BandInterference => "The chosen Wi-Fi band is unavailable. Change the band in Windows hotspot settings.".to_string(),
                TetheringOperationStatus::NetworkLimitedConnectivity => "The shared connection has no internet access.".to_string(),
                _ => format!("Hotspot operation failed ({:?})", status),
            };
            return Err(if extra.is_empty() { why } else { format!("{why} {extra}") });
        }
        Ok(describe(&mgr))
    }

    pub fn configure(ssid: &str, passphrase: &str) -> Result<HotspotInfo, String> {
        let mgr = manager()?;
        let cfg = mgr.GetCurrentAccessPointConfiguration().unwrap_or_else(|_| NetworkOperatorTetheringAccessPointConfiguration::new().expect("config"));
        cfg.SetSsid(&windows::core::HSTRING::from(ssid)).map_err(|e| e.message().to_string())?;
        cfg.SetPassphrase(&windows::core::HSTRING::from(passphrase)).map_err(|e| e.message().to_string())?;
        mgr.ConfigureAccessPointAsync(&cfg).map_err(|e| e.message().to_string())?.join().map_err(|e| e.message().to_string())?;
        Ok(describe(&mgr))
    }
}

#[cfg(not(windows))]
mod imp {
    use super::HotspotInfo;
    const REASON: &str = if cfg!(target_os = "macos") {
        "macOS has no API for Internet Sharing. Turn it on in System Settings → General → Sharing."
    } else {
        "Hotspot control is only automated on Windows. Turn on your hotspot in the system settings."
    };
    pub fn info() -> HotspotInfo {
        HotspotInfo::unsupported(REASON)
    }
    pub fn set_enabled(_on: bool, _preferred: Option<&str>) -> Result<HotspotInfo, String> {
        Err(REASON.to_string())
    }
    pub fn clients() -> Vec<super::HotspotClient> {
        vec![]
    }
    pub fn configure(_ssid: &str, _passphrase: &str) -> Result<HotspotInfo, String> {
        Err(REASON.to_string())
    }
}

/// Initialise the WinRT apartment on the current thread (safe to repeat).
#[cfg(windows)]
pub(crate) fn init_apartment() {
    imp::init_apartment()
}

/// Blocking calls; run them on a blocking thread.
pub fn info() -> HotspotInfo {
    imp::info()
}
pub fn set_enabled(on: bool, preferred: Option<&str>) -> Result<HotspotInfo, String> {
    imp::set_enabled(on, preferred)
}
pub fn clients() -> Vec<HotspotClient> {
    imp::clients()
}
pub fn configure(ssid: &str, passphrase: &str) -> Result<HotspotInfo, String> {
    imp::configure(ssid, passphrase)
}

/// URL that opens the OS hotspot / sharing settings page.
pub fn settings_url() -> &'static str {
    if cfg!(windows) {
        "ms-settings:network-mobilehotspot"
    } else if cfg!(target_os = "macos") {
        "x-apple.systempreferences:com.apple.preferences.sharing?Internet_Sharing"
    } else {
        ""
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostnames_split_into_name_and_ip() {
        // HostNameType: 0 DomainName, 1 Ipv4, 2 Ipv6, 3 Bluetooth.
        let (name, ip) = split_hostnames(&[(1, "192.168.137.23".into()), (2, "fe80::1".into())]);
        assert_eq!(name, None);
        assert_eq!(ip.as_deref(), Some("192.168.137.23"));
        let (name, ip) = split_hostnames(&[(0, "Galaxy-S24".into()), (1, "192.168.137.41".into())]);
        assert_eq!(name.as_deref(), Some("Galaxy-S24"));
        assert_eq!(ip.as_deref(), Some("192.168.137.41"));
        assert_eq!(split_hostnames(&[]), (None, None));
    }

    #[test]
    fn wifi_qr_escapes_specials() {
        assert_eq!(wifi_qr("My Net", Some("p;a,s:s")), "WIFI:T:WPA;S:My Net;P:p\\;a\\,s\\:s;;");
        assert_eq!(wifi_qr("Open", None), "WIFI:T:nopass;S:Open;;");
    }

    #[cfg(not(windows))]
    #[test]
    fn non_windows_is_unsupported() {
        let i = info();
        assert!(!i.supported);
        assert_eq!(i.state, "unsupported");
        assert!(set_enabled(true, None).is_err());
        assert!(clients().is_empty());
    }
}
