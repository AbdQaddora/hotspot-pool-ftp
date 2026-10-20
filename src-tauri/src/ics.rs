//! Internet for hotspot devices: which laptop connection is shared, whether
//! Windows Internet Connection Sharing (ICS) is wired from it to the hotspot
//! adapter, and a one-shot repair (what MyPublicWiFi's "ICS" mode does).

use serde::{Deserialize, Serialize};

/// Windows's own captive-portal probe; opening it brings up the sign-in page.
pub const SIGN_IN_URL: &str = "http://www.msftconnecttest.com/redirect";

/// A laptop connection that could be shared with the hotspot.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Upstream {
    /// Adapter GUID, lowercase, no braces.
    pub id: String,
    pub name: String,
    /// "wifi" | "ethernet" | "cellular" | "other"
    pub kind: &'static str,
    /// "internet" | "signin" | "local" | "none"
    pub connectivity: &'static str,
    /// True for the connection Windows itself treats as the internet one.
    pub is_default: bool,
}

/// One entry of Windows's connection list with its ICS role.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SharingConnection {
    pub id: String,
    pub name: String,
    pub connected: bool,
    /// "public" (shared out) | "private" (receives the share) | "none"
    pub sharing: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SharingState {
    /// False when Windows refused to tell us (not Windows, COM error…).
    pub available: bool,
    pub connections: Vec<SharingConnection>,
    pub public_id: Option<String>,
    pub private_id: Option<String>,
    pub error: Option<String>,
}

impl SharingState {
    pub fn unavailable(err: impl Into<String>) -> Self {
        SharingState { available: false, connections: vec![], public_id: None, private_id: None, error: Some(err.into()) }
    }
}

/// Lowercase GUID without braces, or None when it is not a GUID.
pub fn norm_guid(s: &str) -> Option<String> {
    let g = s.trim().trim_start_matches('{').trim_end_matches('}').to_ascii_lowercase();
    let parts: Vec<&str> = g.split('-').collect();
    let lens = [8, 4, 4, 4, 12];
    if parts.len() != 5 || parts.iter().zip(lens).any(|(p, l)| p.len() != l || !p.chars().all(|c| c.is_ascii_hexdigit())) {
        return None;
    }
    Some(g)
}

/// GUID from its parts, in the normalized form.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn guid_parts_string(d1: u32, d2: u16, d3: u16, d4: [u8; 8]) -> String {
    format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        d1, d2, d3, d4[0], d4[1], d4[2], d4[3], d4[4], d4[5], d4[6], d4[7]
    )
}

#[cfg(windows)]
pub fn guid_string(g: &windows::core::GUID) -> String {
    guid_parts_string(g.data1, g.data2, g.data3, g.data4)
}

fn braced_upper(norm: &str) -> String {
    format!("{{{}}}", norm.to_ascii_uppercase())
}

#[derive(Deserialize)]
struct RawConnection {
    id: String,
    name: String,
    #[serde(default)]
    connected: bool,
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    r#type: i32,
}

/// Parse the JSON the read script prints. A bare object (one connection) and
/// an empty output (no connections) are both accepted; anything else means
/// Windows refused.
pub fn parse_sharing_json(json: &str) -> SharingState {
    let text = json.trim();
    if text.is_empty() {
        return SharingState::unavailable("Windows printed no sharing state");
    }
    let raws: Vec<RawConnection> = match serde_json::from_str::<Vec<RawConnection>>(text) {
        Ok(v) => v,
        Err(_) => match serde_json::from_str::<RawConnection>(text) {
            Ok(one) => vec![one],
            Err(_) => return SharingState::unavailable(crate::elevate::first_line(text).chars().take(200).collect::<String>()),
        },
    };
    let mut st = SharingState { available: true, connections: vec![], public_id: None, private_id: None, error: None };
    for r in raws {
        let id = norm_guid(&r.id).unwrap_or(r.id);
        let sharing = match (r.enabled, r.r#type) {
            (true, 0) => "public",
            (true, 1) => "private",
            _ => "none",
        };
        if sharing == "public" {
            st.public_id = Some(id.clone());
        } else if sharing == "private" {
            st.private_id = Some(id.clone());
        }
        st.connections.push(SharingConnection { id, name: r.name, connected: r.connected, sharing });
    }
    st
}

/// PowerShell that prints every connection and its ICS role as JSON.
pub fn read_script() -> &'static str {
    concat!(
        "$ErrorActionPreference = 'Stop'\r\n",
        "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8\r\n",
        "$m = New-Object -ComObject HNetCfg.HNetShare\r\n",
        "$out = @()\r\n",
        "foreach ($c in $m.EnumEveryConnection) {\r\n",
        "  $p = $m.NetConnectionProps.Invoke($c)\r\n",
        "  $cfg = $m.INetSharingConfigurationForINetConnection.Invoke($c)\r\n",
        "  $out += [pscustomobject]@{ id = [string]$p.Guid; name = [string]$p.Name; connected = ($p.Status -eq 2); enabled = [bool]$cfg.SharingEnabled; type = [int]$cfg.SharingConnectionType }\r\n",
        "}\r\n",
        "ConvertTo-Json -InputObject @($out) -Compress\r\n",
    )
}

/// PowerShell that makes `public` the shared connection and `private` the
/// hotspot side. Both ids must be GUIDs; nothing else is ever interpolated.
///
/// Order matters, learned the hard way: `EnableSharing` fails with
/// 0x80040201 ("An event was unable to invoke any of the subscribers") when
/// `root\Microsoft\HomeNet` still holds IsIcsPublic/IsIcsPrivate flags for
/// adapters that no longer exist (KB 828807) — and Mobile Hotspot creates a
/// fresh virtual adapter on every start, so those pile up. The script clears
/// them, makes sure the ICS service runs, enables public then private, and
/// retries after another cleanup if Windows still refuses. `$step` names the
/// stage for the error file.
pub fn apply_script(public: &str, private: &str) -> Result<String, String> {
    let pub_id = norm_guid(public).ok_or_else(|| format!("Not an adapter id: {public}"))?;
    let priv_id = norm_guid(private).ok_or_else(|| format!("Not an adapter id: {private}"))?;
    const TEMPLATE: &str = concat!(
        "$step = 'start'\r\n",
        "$m = New-Object -ComObject HNetCfg.HNetShare\r\n",
        "$conns = @($m.EnumEveryConnection)\r\n",
        "$cfgOf = { param($c) $m.INetSharingConfigurationForINetConnection.Invoke($c) }\r\n",
        "$propsOf = { param($c) $m.NetConnectionProps.Invoke($c) }\r\n",
        "function Clear-StaleIcs {\r\n",
        "  # KB 828807: stale IsIcsPublic/IsIcsPrivate flags make EnableSharing fail with 0x80040201.\r\n",
        "  foreach ($e in @(Get-WmiObject -Namespace 'root\\Microsoft\\HomeNet' -Class HNet_ConnectionProperties -ErrorAction SilentlyContinue)) {\r\n",
        "    if ($e.IsIcsPublic -or $e.IsIcsPrivate) { try { $e.IsIcsPublic = $false; $e.IsIcsPrivate = $false; $e.Put() | Out-Null } catch {} }\r\n",
        "  }\r\n",
        "}\r\n",
        "$step = 'ICS service'\r\n",
        "try {\r\n",
        "  $svc = Get-Service SharedAccess -ErrorAction Stop\r\n",
        "  if ($svc.StartType -eq 'Disabled') { Set-Service SharedAccess -StartupType Manual }\r\n",
        "  if ($svc.Status -ne 'Running') { Start-Service SharedAccess }\r\n",
        "} catch {}\r\n",
        "$step = 'disable old sharing'\r\n",
        "# Disabling the public side also clears its private peer; a stale 'enabled' on the peer may then throw.\r\n",
        "foreach ($c in $conns) { $cfg = & $cfgOf $c; if ($cfg.SharingEnabled) { try { $cfg.DisableSharing() } catch {} } }\r\n",
        "$step = 'clear stale flags'\r\n",
        "Clear-StaleIcs\r\n",
        "$step = 'find connections'\r\n",
        "$pub = $conns | Where-Object { (& $propsOf $_).Guid -eq '__PUB__' } | Select-Object -First 1\r\n",
        "$priv = $conns | Where-Object { (& $propsOf $_).Guid -eq '__PRIV__' } | Select-Object -First 1\r\n",
        "if (-not $pub) { throw 'Shared connection not found' }\r\n",
        "if (-not $priv) { throw 'Hotspot adapter not found' }\r\n",
        "$attempt = 0\r\n",
        "while ($true) {\r\n",
        "  $attempt++\r\n",
        "  try {\r\n",
        "    $step = \"enable public (attempt $attempt)\"\r\n",
        "    (& $cfgOf $pub).EnableSharing(0)\r\n",
        "    $step = \"enable private (attempt $attempt)\"\r\n",
        "    (& $cfgOf $priv).EnableSharing(1)\r\n",
        "    break\r\n",
        "  } catch {\r\n",
        "    if ($attempt -ge 3) { throw $_.Exception.Message }\r\n",
        "    foreach ($c in $conns) { try { (& $cfgOf $c).DisableSharing() } catch {} }\r\n",
        "    Clear-StaleIcs\r\n",
        "    Start-Sleep -Seconds 2\r\n",
        "  }\r\n",
        "}\r\n",
    );
    Ok(TEMPLATE.replace("__PUB__", &braced_upper(&pub_id)).replace("__PRIV__", &braced_upper(&priv_id)))
}

// ------------------------------------------------------------ live queries

/// Connections the laptop could share, in Windows's order.
pub fn list_upstreams() -> Vec<Upstream> {
    #[cfg(windows)]
    {
        use windows::Networking::Connectivity::{NetworkConnectivityLevel, NetworkInformation};
        let default_id = NetworkInformation::GetInternetConnectionProfile()
            .ok()
            .and_then(|p| p.NetworkAdapter().ok())
            .and_then(|a| a.NetworkAdapterId().ok())
            .map(|g| guid_string(&g));
        let Ok(profiles) = NetworkInformation::GetConnectionProfiles() else { return vec![] };
        let mut out = Vec::new();
        for p in profiles {
            let Ok(adapter) = p.NetworkAdapter() else { continue };
            let Ok(id) = adapter.NetworkAdapterId().map(|g| guid_string(&g)) else { continue };
            let kind = match adapter.IanaInterfaceType().unwrap_or(0) {
                6 => "ethernet",
                71 => "wifi",
                243 | 244 => "cellular",
                _ => "other",
            };
            let connectivity = match p.GetNetworkConnectivityLevel() {
                Ok(NetworkConnectivityLevel::InternetAccess) => "internet",
                Ok(NetworkConnectivityLevel::ConstrainedInternetAccess) => "signin",
                Ok(NetworkConnectivityLevel::LocalAccess) => "local",
                _ => "none",
            };
            let name = p.ProfileName().map(|s| s.to_string()).unwrap_or_default();
            let is_default = default_id.as_deref() == Some(id.as_str());
            if out.iter().any(|u: &Upstream| u.id == id) {
                continue;
            }
            out.push(Upstream { id, name: if name.is_empty() { kind.to_string() } else { name }, kind, connectivity, is_default });
        }
        out
    }
    #[cfg(not(windows))]
    {
        vec![]
    }
}

/// Current ICS pairing as Windows reports it (hidden PowerShell, no UAC).
pub fn sharing_state() -> SharingState {
    match crate::elevate::run_hidden(read_script()) {
        Ok(out) => parse_sharing_json(&out),
        Err(e) => SharingState::unavailable(if e.is_empty() { "Windows did not report the sharing state".to_string() } else { e }),
    }
}

/// A value plus when it was computed; `get` recomputes when asked for a
/// fresh one or when the value is older than `max_age`.
pub struct Cached<T> {
    pub value: Option<T>,
    pub stamp: Option<std::time::Instant>,
}

impl<T> Default for Cached<T> {
    fn default() -> Self {
        Cached { value: None, stamp: None }
    }
}

impl<T: Clone> Cached<T> {
    pub fn get(&mut self, fresh: bool, max_age: std::time::Duration, compute: impl FnOnce() -> T) -> T {
        let stale = self.stamp.map(|t| t.elapsed() > max_age).unwrap_or(true);
        if fresh || stale || self.value.is_none() {
            self.value = Some(compute());
            self.stamp = Some(std::time::Instant::now());
        }
        self.value.clone().expect("just set")
    }
}

/// Reading ICS state spawns a PowerShell process, so timer polls reuse a
/// recent answer; `fresh` forces a new read (after a change or a click).
pub fn sharing_state_cached(fresh: bool) -> SharingState {
    use std::sync::Mutex;
    static CACHE: Mutex<Option<Cached<SharingState>>> = Mutex::new(None);
    let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    guard.get_or_insert_with(Cached::default).get(fresh, std::time::Duration::from_secs(20), sharing_state)
}

/// The hotspot's own adapter shows up as a profile too; it can't be its own upstream.
pub fn without_adapter(upstreams: Vec<Upstream>, adapter_id: Option<&str>) -> Vec<Upstream> {
    match adapter_id {
        Some(id) => upstreams.into_iter().filter(|u| u.id != id).collect(),
        None => upstreams,
    }
}

/// Point ICS from `public` to `private` (one UAC prompt).
pub fn apply_sharing(public: &str, private: &str) -> Result<(), String> {
    let script = apply_script(public, private)?;
    crate::elevate::run_elevated("apply-ics.ps1", &script).map_err(|e| {
        if e.contains("cancelled") {
            "Windows needs permission to change Internet Connection Sharing. The prompt was cancelled.".to_string()
        } else {
            format!("Could not set up Internet Connection Sharing: {e}")
        }
    })
}

/// The upstream that is (or would be) shared: the selected one when it is
/// still connected, else Windows's default, else any connection with
/// internet, else the first.
pub fn effective_upstream<'a>(selected: Option<&str>, upstreams: &'a [Upstream]) -> Option<&'a Upstream> {
    selected
        .and_then(|id| upstreams.iter().find(|u| u.id == id))
        .or_else(|| upstreams.iter().find(|u| u.is_default))
        .or_else(|| upstreams.iter().find(|u| u.connectivity == "internet"))
        .or_else(|| upstreams.first())
}

/// Everything the Share Wi-Fi tab shows, in one snapshot.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WifiShareStatus {
    pub hotspot: crate::hotspot::HotspotInfo,
    pub upstreams: Vec<Upstream>,
    /// What the user picked (settings); None = automatic.
    pub selected_upstream: Option<String>,
    /// What is actually used, after fallbacks.
    pub effective_upstream: Option<String>,
    pub sharing: SharingState,
    pub hotspot_adapter: Option<crate::network::NetInterface>,
    pub clients: Vec<crate::hotspot::HotspotClient>,
    pub health: &'static str,
    pub sign_in_url: &'static str,
}

/// Blocking: queries Windows (WinRT + one hidden PowerShell unless a recent
/// ICS answer is reused). Run off the UI thread.
pub fn snapshot(selected: Option<&str>, fresh: bool) -> WifiShareStatus {
    let hotspot = crate::hotspot::info();
    let hotspot_adapter = crate::network::list_interfaces().into_iter().find(|i| i.is_hotspot);
    let upstreams = without_adapter(list_upstreams(), hotspot_adapter.as_ref().and_then(|a| a.adapter_id.as_deref()));
    let effective = effective_upstream(selected, &upstreams).cloned();
    let sharing = sharing_state_cached(fresh);
    let clients = if hotspot.state == "on" { crate::hotspot::clients() } else { vec![] };
    let health = assess(hotspot.state == "on", effective.as_ref(), &sharing, hotspot_adapter.as_ref().and_then(|a| a.adapter_id.as_deref()));
    WifiShareStatus {
        hotspot,
        upstreams,
        selected_upstream: selected.map(String::from),
        effective_upstream: effective.map(|u| u.id),
        sharing,
        hotspot_adapter,
        clients,
        health,
        sign_in_url: SIGN_IN_URL,
    }
}

/// Health of internet sharing for the devices. Exact strings are part of the
/// UI contract: sharing | not_shared | wrong_upstream | signin | no_internet |
/// hotspot_off | unknown.
pub fn assess(hotspot_on: bool, upstream: Option<&Upstream>, sharing: &SharingState, hotspot_adapter: Option<&str>) -> &'static str {
    if !hotspot_on {
        return "hotspot_off";
    }
    let Some(up) = upstream else { return "no_internet" };
    match up.connectivity {
        "internet" => {}
        "signin" => return "signin",
        _ => return "no_internet",
    }
    if !sharing.available {
        return "unknown";
    }
    match sharing.public_id.as_deref() {
        None => return "not_shared",
        Some(p) if p != up.id => return "wrong_upstream",
        Some(_) => {}
    }
    let Some(adapter) = hotspot_adapter else { return "unknown" };
    if sharing.private_id.as_deref() != Some(adapter) {
        return "not_shared";
    }
    "sharing"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn up(id: &str, connectivity: &'static str) -> Upstream {
        Upstream { id: id.into(), name: "Wi-Fi".into(), kind: "wifi", connectivity, is_default: true }
    }
    fn sharing(public: Option<&str>, private: Option<&str>) -> SharingState {
        SharingState { available: true, connections: vec![], public_id: public.map(String::from), private_id: private.map(String::from), error: None }
    }
    const A: &str = "abcdef01-1234-5678-9abc-def012345678";
    const B: &str = "11111111-2222-3333-4444-555555555555";
    const H: &str = "99999999-8888-7777-6666-555555555555";

    #[test]
    fn norm_guid_forms() {
        assert_eq!(norm_guid("{ABCDEF01-1234-5678-9ABC-DEF012345678}").as_deref(), Some(A));
        assert_eq!(norm_guid(A).as_deref(), Some(A));
        assert_eq!(norm_guid("nope"), None);
        assert_eq!(norm_guid("abcdef01-1234-5678-9abc-def01234567g"), None);
    }

    #[test]
    fn guid_parts_string_formats_lowercase() {
        assert_eq!(guid_parts_string(0xabcdef01, 0x1234, 0x5678, [0x9a, 0xbc, 0xde, 0xf0, 0x12, 0x34, 0x56, 0x78]), A);
    }

    #[test]
    fn parse_sharing_sets_public_and_private() {
        let json = r#"[{"id":"{ABCDEF01-1234-5678-9ABC-DEF012345678}","name":"Wi-Fi","connected":true,"enabled":true,"type":0},
                       {"id":"{99999999-8888-7777-6666-555555555555}","name":"Local Area Connection* 12","connected":true,"enabled":true,"type":1},
                       {"id":"{11111111-2222-3333-4444-555555555555}","name":"Ethernet","connected":false,"enabled":false,"type":0}]"#;
        let s = parse_sharing_json(json);
        assert!(s.available);
        assert_eq!(s.public_id.as_deref(), Some(A));
        assert_eq!(s.private_id.as_deref(), Some(H));
        assert_eq!(s.connections.len(), 3);
        assert_eq!(s.connections[2].sharing, "none");
        assert_eq!(s.connections[1].sharing, "private");
        assert!(!s.connections[2].connected);
    }

    #[test]
    fn parse_single_object_and_empty() {
        // ConvertTo-Json emits a bare object for a one-element array on old PowerShell.
        let s = parse_sharing_json(r#"{"id":"{ABCDEF01-1234-5678-9ABC-DEF012345678}","name":"Wi-Fi","connected":true,"enabled":false,"type":0}"#);
        assert!(s.available);
        assert_eq!(s.connections.len(), 1);
        assert_eq!(s.public_id, None);
        // The read script always prints at least "[]": empty output means it did not run.
        let e = parse_sharing_json("");
        assert!(!e.available);
        assert!(e.error.is_some());
    }

    #[test]
    fn parse_rejects_garbage() {
        let s = parse_sharing_json("Access denied\r\nAt C:\\x.ps1:2 char:6\r\n+ ~~~");
        assert!(!s.available);
        assert_eq!(s.error.as_deref(), Some("Access denied"));
    }

    #[test]
    fn scripts_are_robust() {
        assert!(read_script().contains("[Console]::OutputEncoding"));
        let s = apply_script(A, H).unwrap();
        assert!(s.contains("try { $cfg.DisableSharing() } catch {}"));
    }

    /// HRESULT 0x80040201 from EnableSharing comes from stale IsIcsPublic /
    /// IsIcsPrivate flags in root\Microsoft\HomeNet (KB 828807); the script
    /// clears them before enabling, makes sure the ICS service runs, and
    /// retries once after another cleanup.
    #[test]
    fn apply_script_clears_stale_homenet_flags_first() {
        let s = apply_script(A, H).unwrap();
        let cleanup = s.find("HNet_ConnectionProperties").expect("HomeNet cleanup present");
        assert!(s.contains(r"root\Microsoft\HomeNet"));
        assert!(s.contains("IsIcsPublic") && s.contains("IsIcsPrivate"));
        assert!(cleanup < s.find("EnableSharing(0)").unwrap());
        assert!(s.contains("SharedAccess"));
        assert!(s.contains("0x80040201") || s.contains("80040201"));
        assert!(s.matches("EnableSharing(0)").count() >= 1 && s.contains("$attempt"));
        // Failures name the step so the error file says where it died.
        assert!(s.contains("$step"));
    }

    #[test]
    fn hotspot_adapter_is_not_an_upstream() {
        let list = vec![up(A, "internet"), up(H, "local")];
        let kept = without_adapter(list, Some(H));
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].id, A);
        assert_eq!(without_adapter(vec![up(A, "internet")], None).len(), 1);
    }

    #[test]
    fn cache_reuses_recent_values() {
        let mut c = Cached::<u32>::default();
        let mut calls = 0;
        let mut get = |fresh: bool, c: &mut Cached<u32>| {
            c.get(fresh, std::time::Duration::from_secs(60), || {
                calls += 1;
                calls
            })
        };
        assert_eq!(get(false, &mut c), 1);
        assert_eq!(get(false, &mut c), 1);
        assert_eq!(get(true, &mut c), 2);
        c.stamp = Some(std::time::Instant::now() - std::time::Duration::from_secs(120));
        assert_eq!(get(false, &mut c), 3);
    }

    #[test]
    fn script_rejects_bad_guid() {
        assert!(apply_script("x", "y").is_err());
        assert!(apply_script(A, "y").is_err());
        let s = apply_script(A, H).unwrap();
        assert!(s.contains("{ABCDEF01-1234-5678-9ABC-DEF012345678}"));
        assert!(s.contains("{99999999-8888-7777-6666-555555555555}"));
        assert!(s.find("EnableSharing(0)").unwrap() < s.find("EnableSharing(1)").unwrap());
        assert!(s.contains("DisableSharing()"));
        assert!(read_script().contains("HNetCfg.HNetShare"));
    }

    #[test]
    fn assess_matrix() {
        let wifi = up(A, "internet");
        assert_eq!(assess(false, Some(&wifi), &sharing(Some(A), Some(H)), Some(H)), "hotspot_off");
        assert_eq!(assess(true, None, &sharing(Some(A), Some(H)), Some(H)), "no_internet");
        assert_eq!(assess(true, Some(&up(A, "signin")), &sharing(Some(A), Some(H)), Some(H)), "signin");
        assert_eq!(assess(true, Some(&up(A, "local")), &sharing(Some(A), Some(H)), Some(H)), "no_internet");
        let unavailable = SharingState { available: false, ..sharing(None, None) };
        assert_eq!(assess(true, Some(&wifi), &unavailable, Some(H)), "unknown");
        assert_eq!(assess(true, Some(&wifi), &sharing(None, None), Some(H)), "not_shared");
        assert_eq!(assess(true, Some(&wifi), &sharing(Some(B), Some(H)), Some(H)), "wrong_upstream");
        assert_eq!(assess(true, Some(&wifi), &sharing(Some(A), Some(H)), Some(H)), "sharing");
    }

    #[test]
    fn effective_upstream_rules() {
        let eth = Upstream { id: B.into(), name: "Ethernet".into(), kind: "ethernet", connectivity: "internet", is_default: false };
        let wifi = Upstream { id: A.into(), name: "Wi-Fi".into(), kind: "wifi", connectivity: "internet", is_default: true };
        let local = Upstream { id: H.into(), name: "Lab".into(), kind: "other", connectivity: "local", is_default: false };
        let list = vec![local.clone(), eth.clone(), wifi.clone()];
        assert_eq!(effective_upstream(Some(B), &list).map(|u| u.id.as_str()), Some(B));
        assert_eq!(effective_upstream(Some("not-there"), &list).map(|u| u.id.as_str()), Some(A));
        assert_eq!(effective_upstream(None, &list).map(|u| u.id.as_str()), Some(A));
        let no_default = vec![local.clone(), eth.clone()];
        assert_eq!(effective_upstream(None, &no_default).map(|u| u.id.as_str()), Some(B));
        assert_eq!(effective_upstream(None, &[local.clone()]).map(|u| u.id.as_str()), Some(H));
        assert_eq!(effective_upstream(None, &[]), None);
    }

    #[test]
    fn assess_private_unknown() {
        let wifi = up(A, "internet");
        assert_eq!(assess(true, Some(&wifi), &sharing(Some(A), Some(H)), None), "unknown");
        assert_eq!(assess(true, Some(&wifi), &sharing(Some(A), Some(B)), Some(H)), "not_shared");
        assert_eq!(assess(true, Some(&wifi), &sharing(Some(A), None), Some(H)), "not_shared");
    }
}
