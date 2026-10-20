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
pub fn abort_reason(error_code: i32) -> &'static str {
    match error_code {
        1 => "Wi-Fi is turned off. Turn Wi-Fi on first.",
        2 => "Another hotspot program is using the Wi-Fi adapter. Turn off Windows Mobile Hotspot and MyPublicWiFi, then try again.",
        _ => "Windows could not start the access point.",
    }
}

/// Lists the Wi-Fi Direct virtual adapter that is up (the group owner side).
pub fn go_adapter_script() -> &'static str {
    concat!(
        "$ErrorActionPreference = 'Stop'\r\n",
        "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8\r\n",
        "$a = Get-NetAdapter -IncludeHidden | Where-Object { $_.InterfaceDescription -like 'Microsoft Wi-Fi Direct Virtual Adapter*' -and $_.Status -eq 'Up' } | Select-Object Name, InterfaceGuid, Status\r\n",
        "ConvertTo-Json -InputObject @($a) -Compress\r\n",
    )
}

#[derive(Deserialize)]
struct RawAdapter {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "InterfaceGuid")]
    guid: String,
}

/// (friendly name, normalized GUID) of the first adapter in the script output.
pub fn pick_go_adapter(json: &str) -> Option<(String, String)> {
    let text = json.trim();
    if text.is_empty() {
        return None;
    }
    let list: Vec<RawAdapter> = serde_json::from_str::<Vec<RawAdapter>>(text)
        .or_else(|_| serde_json::from_str::<RawAdapter>(text).map(|a| vec![a]))
        .ok()?;
    list.into_iter().find_map(|a| crate::ics::norm_guid(&a.guid).map(|g| (a.name, g)))
}

/// Ask Windows which Wi-Fi Direct virtual adapter is up right now.
pub fn find_go_adapter() -> Result<(String, String), String> {
    let out = crate::elevate::run_hidden(go_adapter_script())?;
    pick_go_adapter(&out).ok_or_else(|| "Windows has not brought up the access point's network adapter yet.".to_string())
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

    pub fn stop() {
        let Some(ap) = lock(&AP).take() else { return };
        let _ = ap.publisher.RemoveStatusChanged(ap.status_token);
        let _ = ap.listener.RemoveConnectionRequested(ap.request_token);
        let _ = ap.publisher.Stop();
        let clients: Vec<Client> = lock(&ap.shared).clients.drain().map(|(_, c)| c).collect();
        for c in clients {
            let _ = c.device.RemoveConnectionStatusChanged(c.token);
            let _ = c.device.Close();
        }
    }

    pub fn info() -> ApInfo {
        let guard = lock(&AP);
        let Some(ap) = guard.as_ref() else { return ApInfo::default() };
        let g = lock(&ap.shared);
        ApInfo {
            running: g.status == STARTED,
            ssid: Some(ap.ssid.clone()),
            passphrase: Some(ap.passphrase.clone()),
            clients: g.clients.values().map(|c| HotspotClient { mac: String::new(), name: c.name.clone(), ip: c.ip.clone() }).collect(),
            error: g.error.clone(),
        }
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

    #[test]
    fn pick_go_adapter_parses_array_object_and_empty() {
        let two = r#"[{"Name":"Local Area Connection* 2","InterfaceGuid":"{99999999-8888-7777-6666-555555555555}","Status":"Up"},
                      {"Name":"Local Area Connection* 1","InterfaceGuid":"{11111111-2222-3333-4444-555555555555}","Status":"Up"}]"#;
        assert_eq!(pick_go_adapter(two), Some(("Local Area Connection* 2".into(), "99999999-8888-7777-6666-555555555555".into())));
        let one = r#"{"Name":"Local Area Connection* 12","InterfaceGuid":"{ABCDEF01-1234-5678-9ABC-DEF012345678}","Status":"Up"}"#;
        assert_eq!(pick_go_adapter(one), Some(("Local Area Connection* 12".into(), "abcdef01-1234-5678-9abc-def012345678".into())));
        assert_eq!(pick_go_adapter(""), None);
        assert_eq!(pick_go_adapter("[]"), None);
        assert_eq!(pick_go_adapter("Access denied"), None);
        assert!(go_adapter_script().contains("Microsoft Wi-Fi Direct Virtual Adapter"));
        assert!(go_adapter_script().contains("-IncludeHidden"));
    }

    #[cfg(not(windows))]
    #[test]
    fn non_windows_stub() {
        assert!(start("x", "12345678").is_err());
        assert!(!info().running);
        stop();
    }
}
