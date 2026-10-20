//! Our own Wi-Fi access point through the WinRT Wi-Fi Direct API in legacy
//! mode — the way MyPublicWiFi creates its hotspot — so that classic Windows
//! Internet Connection Sharing can route the laptop's internet to it. (Windows
//! Mobile Hotspot forwards through WinNAT instead and refuses classic ICS.)
//!
//! Sequence (Microsoft's WiFiDirectLegacyAPDemo): publisher with autonomous
//! group owner + legacy SSID/passphrase, `StatusChanged` for Started/Aborted,
//! a connection listener that accepts every request with
//! `WiFiDirectDevice::FromIdAsync` and keeps the device alive while connected.

use crate::hotspot::HotspotClient;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApInfo {
    pub running: bool,
    pub ssid: Option<String>,
    pub passphrase: Option<String>,
    pub clients: Vec<HotspotClient>,
    pub error: Option<String>,
}

/// `WiFiDirectError`: 0 Success, 1 RadioNotAvailable, 2 ResourceInUse.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn abort_reason(error_code: i32) -> &'static str {
    match error_code {
        1 => "Wi-Fi is turned off. Turn Wi-Fi on first.",
        2 => "Another hotspot program is using the Wi-Fi adapter. Turn off Windows Mobile Hotspot and MyPublicWiFi, then try again.",
        _ => "Windows could not start the access point.",
    }
}

/// Why starting the access point engine failed, and whether Windows Mobile
/// Hotspot may be used instead. Once Internet Connection Sharing has been
/// touched, starting Mobile Hotspot on top of it recreates the ICS/WinNAT
/// conflict, so only failures before that point allow the fallback.
#[derive(Debug, Clone, PartialEq)]
pub enum StartFail {
    /// Mobile Hotspot could not be turned off, the AP did not start, or its adapter never appeared.
    ApStart(String),
    /// The AP is up but sharing could not be set up (UAC cancelled, ICS error, no address).
    AfterAp(String),
}

impl StartFail {
    pub fn allows_mobile_fallback(&self) -> bool {
        matches!(self, StartFail::ApStart(_))
    }
    pub fn message(&self) -> &str {
        match self {
            StartFail::ApStart(m) | StartFail::AfterAp(m) => m,
        }
    }
}

/// The abort reason Windows gives while the previous hotspot is still letting go of the radio.
pub fn is_resource_in_use(message: &str) -> bool {
    message == abort_reason(2)
}

/// 12 random alphanumerics, used only when Windows has no saved hotspot password.
pub fn random_passphrase() -> String {
    use rand::Rng;
    rand::rng().sample_iter(&rand::distr::Alphanumeric).take(12).map(char::from).collect()
}

/// Lists every Wi-Fi Direct virtual adapter with its status (Windows ships
/// two; the group owner's one comes Up when the AP starts).
pub fn go_adapter_script() -> &'static str {
    concat!(
        "$ErrorActionPreference = 'Stop'\r\n",
        "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8\r\n",
        "$a = Get-NetAdapter -IncludeHidden | Where-Object { $_.InterfaceDescription -like 'Microsoft Wi-Fi Direct Virtual Adapter*' } | Select-Object Name, InterfaceGuid, Status\r\n",
        "ConvertTo-Json -InputObject @($a) -Compress\r\n",
    )
}

#[derive(Deserialize)]
struct RawAdapter {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "InterfaceGuid")]
    guid: String,
    #[serde(rename = "Status", default)]
    status: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WfdAdapter {
    pub name: String,
    /// Normalized GUID.
    pub guid: String,
    pub up: bool,
    pub status: String,
}

/// Parse the script output (array, bare object, or nothing).
pub fn list_wfd_adapters(json: &str) -> Vec<WfdAdapter> {
    let text = json.trim();
    if text.is_empty() {
        return vec![];
    }
    let Ok(list) = serde_json::from_str::<Vec<RawAdapter>>(text).or_else(|_| serde_json::from_str::<RawAdapter>(text).map(|a| vec![a])) else { return vec![] };
    list.into_iter()
        .filter_map(|a| crate::ics::norm_guid(&a.guid).map(|g| WfdAdapter { name: a.name, guid: g, up: a.status.eq_ignore_ascii_case("Up"), status: a.status }))
        .collect()
}

/// The group owner's adapter: one that came Up since `before`, else any Up one.
pub fn pick_go_adapter(before: &[WfdAdapter], after: &[WfdAdapter]) -> Option<WfdAdapter> {
    let was_up = |g: &str| before.iter().any(|b| b.guid == g && b.up);
    after.iter().find(|a| a.up && !was_up(&a.guid)).or_else(|| after.iter().find(|a| a.up)).cloned()
}

/// Ask Windows for the current list of Wi-Fi Direct virtual adapters.
pub fn wfd_adapters() -> Vec<WfdAdapter> {
    crate::elevate::run_hidden(go_adapter_script()).map(|out| list_wfd_adapters(&out)).unwrap_or_default()
}

/// One-line description of candidates, for the log when none came up.
pub fn describe_adapters(list: &[WfdAdapter]) -> String {
    if list.is_empty() {
        return "none found".into();
    }
    list.iter().map(|a| format!("{} ({})", a.name, a.status)).collect::<Vec<_>>().join(", ")
}

#[cfg(windows)]
mod imp {
    use super::{abort_reason, ApInfo};
    use crate::hotspot::HotspotClient;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use windows::core::{IInspectable, Ref, HSTRING};
    use windows::Devices::WiFiDirect::{
        WiFiDirectAdvertisementPublisher, WiFiDirectAdvertisementPublisherStatusChangedEventArgs, WiFiDirectConnectionListener, WiFiDirectConnectionRequestedEventArgs, WiFiDirectConnectionStatus, WiFiDirectDevice,
    };
    use windows::Foundation::TypedEventHandler;

    const STARTED: i32 = 1;
    const ABORTED: i32 = 3;

    struct Client {
        device: WiFiDirectDevice,
        token: i64,
        name: Option<String>,
        ip: Option<String>,
    }

    #[derive(Default)]
    struct Shared {
        status: i32,
        error: Option<String>,
        clients: HashMap<String, Client>,
    }

    struct Ap {
        publisher: WiFiDirectAdvertisementPublisher,
        listener: WiFiDirectConnectionListener,
        status_token: i64,
        request_token: i64,
        ssid: String,
        passphrase: String,
        shared: Arc<Mutex<Shared>>,
    }

    static AP: Mutex<Option<Ap>> = Mutex::new(None);

    fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
        m.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn msg(e: windows::core::Error) -> String {
        e.message().to_string()
    }

    /// Runs on its own thread: accepting is what lets a legacy client in.
    fn accept(shared: Arc<Mutex<Shared>>, id: HSTRING, name: Option<String>) {
        crate::hotspot::init_apartment();
        let Ok(op) = WiFiDirectDevice::FromIdAsync(&id) else { return };
        let Ok(device) = op.join() else { return };
        let ip = device
            .GetConnectionEndpointPairs()
            .ok()
            .and_then(|v| v.into_iter().next())
            .and_then(|p| p.RemoteHostName().ok())
            .and_then(|h| h.DisplayName().ok())
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty());
        let key = id.to_string();
        let sh = shared.clone();
        let k = key.clone();
        let token = device
            .ConnectionStatusChanged(&TypedEventHandler::new(move |d: Ref<WiFiDirectDevice>, _: Ref<IInspectable>| {
                let gone = d.as_ref().map(|d| d.ConnectionStatus().map(|s| s == WiFiDirectConnectionStatus::Disconnected).unwrap_or(true)).unwrap_or(true);
                if gone {
                    lock(&sh).clients.remove(&k);
                }
                Ok(())
            }))
            .unwrap_or(0);
        let already_gone = device.ConnectionStatus().map(|s| s == WiFiDirectConnectionStatus::Disconnected).unwrap_or(false);
        if already_gone {
            let _ = device.RemoveConnectionStatusChanged(token);
            return;
        }
        lock(&shared).clients.insert(key, Client { device, token, name, ip });
    }

    pub fn start(ssid: &str, passphrase: &str) -> Result<ApInfo, String> {
        crate::hotspot::init_apartment();
        stop();
        let shared = Arc::new(Mutex::new(Shared::default()));
        let publisher = WiFiDirectAdvertisementPublisher::new().map_err(msg)?;
        let adv = publisher.Advertisement().map_err(msg)?;
        adv.SetIsAutonomousGroupOwnerEnabled(true).map_err(msg)?;
        let legacy = adv.LegacySettings().map_err(msg)?;
        legacy.SetIsEnabled(true).map_err(msg)?;
        legacy.SetSsid(&HSTRING::from(ssid)).map_err(msg)?;
        legacy.Passphrase().map_err(msg)?.SetPassword(&HSTRING::from(passphrase)).map_err(msg)?;

        let sh = shared.clone();
        let status_token = publisher
            .StatusChanged(&TypedEventHandler::new(move |_: Ref<WiFiDirectAdvertisementPublisher>, args: Ref<WiFiDirectAdvertisementPublisherStatusChangedEventArgs>| {
                if let Some(a) = args.as_ref() {
                    let st = a.Status().map(|s| s.0).unwrap_or(-1);
                    let mut g = lock(&sh);
                    g.status = st;
                    g.error = if st == ABORTED { Some(abort_reason(a.Error().map(|e| e.0).unwrap_or(0)).to_string()) } else { None };
                }
                Ok(())
            }))
            .map_err(msg)?;

        let listener = WiFiDirectConnectionListener::new().map_err(msg)?;
        let sh = shared.clone();
        let request_token = listener
            .ConnectionRequested(&TypedEventHandler::new(move |_: Ref<WiFiDirectConnectionListener>, args: Ref<WiFiDirectConnectionRequestedEventArgs>| {
                let Some(a) = args.as_ref() else { return Ok(()) };
                let info = a.GetConnectionRequest()?.DeviceInformation()?;
                let id = info.Id()?;
                let name = info.Name().ok().map(|s| s.to_string()).filter(|s| !s.is_empty());
                let sh = sh.clone();
                std::thread::spawn(move || accept(sh, id, name));
                Ok(())
            }))
            .map_err(msg)?;

        publisher.Start().map_err(msg)?;
        let ap = Ap { publisher, listener, status_token, request_token, ssid: ssid.to_string(), passphrase: passphrase.to_string(), shared: shared.clone() };
        *lock(&AP) = Some(ap);

        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let (st, err) = {
                let g = lock(&shared);
                (g.status, g.error.clone())
            };
            if st == STARTED {
                return Ok(info());
            }
            if st == ABORTED {
                stop();
                return Err(err.unwrap_or_else(|| abort_reason(0).to_string()));
            }
            if Instant::now() >= deadline {
                stop();
                return Err("Windows did not start the access point in time.".into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    const STOPPED: i32 = 2;

    /// Stop the AP and wait (≤ 5 s) until Windows reports it stopped, so the
    /// radio is free for whatever starts next.
    pub fn stop() {
        let Some(ap) = lock(&AP).take() else { return };
        let _ = ap.listener.RemoveConnectionRequested(ap.request_token);
        let was_started = lock(&ap.shared).status == STARTED;
        let _ = ap.publisher.Stop();
        if was_started {
            let deadline = Instant::now() + Duration::from_secs(5);
            while lock(&ap.shared).status != STOPPED && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        let _ = ap.publisher.RemoveStatusChanged(ap.status_token);
        let clients: Vec<Client> = lock(&ap.shared).clients.drain().map(|(_, c)| c).collect();
        for c in clients {
            let _ = c.device.RemoveConnectionStatusChanged(c.token);
            let _ = c.device.Close();
        }
    }

    pub fn info() -> ApInfo {
        let (snapshot, dead) = {
            let guard = lock(&AP);
            let Some(ap) = guard.as_ref() else { return ApInfo::default() };
            let g = lock(&ap.shared);
            let info = ApInfo {
                running: g.status == STARTED,
                ssid: Some(ap.ssid.clone()),
                passphrase: Some(ap.passphrase.clone()),
                clients: g.clients.values().map(|c| HotspotClient { mac: String::new(), name: c.name.clone(), ip: c.ip.clone() }).collect(),
                error: g.error.clone(),
            };
            (info, g.status == ABORTED || g.status == STOPPED)
        };
        if dead {
            // The AP died on its own (Wi-Fi turned off…): keep the reason, drop the objects.
            stop();
        }
        snapshot
    }
}

#[cfg(not(windows))]
mod imp {
    use super::ApInfo;
    pub fn start(_ssid: &str, _passphrase: &str) -> Result<ApInfo, String> {
        Err("Not supported on this OS".into())
    }
    pub fn stop() {}
    pub fn info() -> ApInfo {
        ApInfo::default()
    }
}

/// Blocking: start the access point and wait until Windows reports it started.
pub fn start(ssid: &str, passphrase: &str) -> Result<ApInfo, String> {
    imp::start(ssid, passphrase)
}
pub fn stop() {
    imp::stop()
}
pub fn info() -> ApInfo {
    imp::info()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abort_reasons() {
        assert!(abort_reason(1).contains("Wi-Fi is turned off"));
        assert!(abort_reason(2).contains("Mobile Hotspot"));
        assert!(abort_reason(2).contains("MyPublicWiFi"));
        assert!(abort_reason(0).contains("could not start"));
        assert!(abort_reason(99).contains("could not start"));
    }

    const A1: &str = r#"{"Name":"Local Area Connection* 1","InterfaceGuid":"{11111111-2222-3333-4444-555555555555}","Status":"Disconnected"}"#;
    const A2_DOWN: &str = r#"{"Name":"Local Area Connection* 2","InterfaceGuid":"{99999999-8888-7777-6666-555555555555}","Status":"Disconnected"}"#;
    const A2_UP: &str = r#"{"Name":"Local Area Connection* 2","InterfaceGuid":"{99999999-8888-7777-6666-555555555555}","Status":"Up"}"#;
    const A1_UP: &str = r#"{"Name":"Local Area Connection* 1","InterfaceGuid":"{11111111-2222-3333-4444-555555555555}","Status":"Up"}"#;

    #[test]
    fn list_wfd_adapters_parses_array_object_and_garbage() {
        let list = list_wfd_adapters(&format!("[{A1},{A2_UP}]"));
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].guid, "99999999-8888-7777-6666-555555555555");
        assert!(list[1].up);
        assert!(!list[0].up);
        assert_eq!(list_wfd_adapters(A1).len(), 1);
        assert!(list_wfd_adapters("").is_empty());
        assert!(list_wfd_adapters("[]").is_empty());
        assert!(list_wfd_adapters("Access denied").is_empty());
        assert!(go_adapter_script().contains("Microsoft Wi-Fi Direct Virtual Adapter"));
        assert!(go_adapter_script().contains("-IncludeHidden"));
        assert!(!go_adapter_script().contains("-eq 'Up'"));
    }

    #[test]
    fn pick_go_adapter_prefers_newly_up_then_any_up() {
        let before = list_wfd_adapters(&format!("[{A1_UP},{A2_DOWN}]"));
        // #1 was already Up for another reason (Miracast); #2 came up for us.
        let after = list_wfd_adapters(&format!("[{A1_UP},{A2_UP}]"));
        assert_eq!(pick_go_adapter(&before, &after).map(|a| a.name), Some("Local Area Connection* 2".into()));
        // Nothing changed but one is Up: take it.
        let after2 = list_wfd_adapters(&format!("[{A1},{A2_UP}]"));
        assert_eq!(pick_go_adapter(&[], &after2).map(|a| a.guid), Some("99999999-8888-7777-6666-555555555555".into()));
        // Nothing Up: none.
        assert!(pick_go_adapter(&before, &list_wfd_adapters(&format!("[{A1},{A2_DOWN}]"))).is_none());
    }

    #[test]
    fn fallback_only_before_sharing_was_touched() {
        assert!(StartFail::ApStart("radio off".into()).allows_mobile_fallback());
        assert!(!StartFail::AfterAp("UAC cancelled".into()).allows_mobile_fallback());
        assert_eq!(StartFail::AfterAp("x".into()).message(), "x");
        assert!(is_resource_in_use(abort_reason(2)));
        assert!(!is_resource_in_use(abort_reason(1)));
    }

    #[test]
    fn random_passphrase_is_long_enough() {
        let p = random_passphrase();
        assert_eq!(p.chars().count(), 12);
        assert!(p.chars().all(|c| c.is_ascii_alphanumeric()));
        assert_ne!(random_passphrase(), p);
    }

    #[cfg(not(windows))]
    #[test]
    fn non_windows_stub() {
        assert!(start("x", "12345678").is_err());
        assert!(!info().running);
        stop();
    }
}
