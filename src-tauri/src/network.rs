//! Local network interface discovery with Wi-Fi hotspot detection.
//!
//! The hotspot adapter is identified heuristically. Every OS uses a
//! well-known gateway address for its built-in hotspot / internet sharing
//! feature, so an exact match on that address is the strongest signal:
//!
//! * Windows "Mobile hotspot" / ICS  -> 192.168.137.1
//! * macOS "Internet Sharing"         -> 192.168.2.1 (bridge100)
//! * Linux NetworkManager hotspot     -> 10.42.0.1
//!
//! Interface names and private ".1" gateway addresses are weaker signals.

use serde::Serialize;
use std::net::Ipv4Addr;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetInterface {
    pub name: String,
    pub ip: String,
    pub is_loopback: bool,
    pub is_hotspot: bool,
    /// 0..=100 confidence that this adapter is the hotspot gateway.
    pub score: u8,
    /// Windows adapter GUID (lowercase, no braces); None on other OSes.
    pub adapter_id: Option<String>,
}

const KNOWN_HOTSPOT_IPS: [Ipv4Addr; 4] = [
    Ipv4Addr::new(192, 168, 137, 1),
    // Wi-Fi Direct group owner's own subnet, used when the access point runs without ICS.
    Ipv4Addr::new(192, 168, 49, 1),
    Ipv4Addr::new(192, 168, 2, 1),
    Ipv4Addr::new(10, 42, 0, 1),
];

/// Scores at or above this come only from a gateway-like address (see `is_hotspot`).
const HOTSPOT_SCORE: u8 = 60;
/// Scores at or above this mean the adapter is *named* like a hotspot adapter,
/// even when it has no usable address yet (169.254.x.x while ICS is starting).
const HOTSPOT_NAME_SCORE: u8 = 50;

fn has_hotspot_name(name: &str) -> bool {
    let lname = name.to_ascii_lowercase();
    lname.contains("hotspot")
        || lname.contains("bridge100")
        || lname.starts_with("ap")
        || lname.contains("local area connection*")
        || lname.contains("wi-fi direct")
        || lname.contains("microsoft wi-fi direct")
}

/// An address a hotspot gateway would hold: a known sharing address, or a
/// private ".1". Link-local (APIPA), loopback and unspecified never qualify.
fn is_gateway_like(ip: Ipv4Addr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() || ip.is_link_local() {
        return false;
    }
    KNOWN_HOTSPOT_IPS.contains(&ip) || (ip.is_private() && ip.octets()[3] == 1)
}

fn score_interface(name: &str, ip: Ipv4Addr) -> u8 {
    if ip.is_loopback() || ip.is_unspecified() {
        return 0;
    }
    if KNOWN_HOTSPOT_IPS.contains(&ip) {
        return 100;
    }
    match (has_hotspot_name(name), is_gateway_like(ip)) {
        (true, true) => 90,
        // Name alone is only a tiebreaker: the adapter may still be waiting for its address.
        (true, false) => HOTSPOT_NAME_SCORE,
        (false, true) => 40,
        (false, false) if ip.is_private() => 20,
        _ => 10,
    }
}

/// True when this adapter can serve devices right now: it holds a gateway-like
/// address *and* looks like a hotspot (known address, or hotspot name + ".1").
fn is_hotspot(name: &str, ip: Ipv4Addr) -> bool {
    is_gateway_like(ip) && score_interface(name, ip) >= HOTSPOT_SCORE
}

fn make(name: String, ip: Ipv4Addr, adapter_id: Option<String>) -> NetInterface {
    NetInterface { is_loopback: ip.is_loopback(), is_hotspot: is_hotspot(&name, ip), score: score_interface(&name, ip), name, ip: ip.to_string(), adapter_id }
}

/// Enumerate IPv4 interfaces, highest hotspot confidence first.
pub fn list_interfaces() -> Vec<NetInterface> {
    let mut out: Vec<NetInterface> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|iface| match iface.addr {
            if_addrs::IfAddr::V4(v4) => {
                #[cfg(windows)]
                let adapter_id = crate::ics::norm_guid(&iface.adapter_name);
                #[cfg(not(windows))]
                let adapter_id = None;
                Some(make(iface.name, v4.ip, adapter_id))
            }
            _ => None,
        })
        .collect();
    out.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.name.cmp(&b.name)));
    out
}

/// The adapter internet sharing should target: a ready hotspot gateway if there
/// is one, else an adapter merely *named* like one (e.g. still on 169.254.x.x,
/// which "Fix sharing" needs to find). `list` must be sorted as `list_interfaces` returns it.
pub fn hotspot_adapter_candidate(list: Vec<NetInterface>) -> Option<NetInterface> {
    list.into_iter().find(|i| !i.is_loopback && (i.is_hotspot || i.score >= HOTSPOT_NAME_SCORE))
}

/// Best guess for the hotspot adapter: the highest scoring non-loopback IPv4.
pub fn detect_hotspot() -> Option<NetInterface> {
    list_interfaces().into_iter().find(|i| !i.is_loopback)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_hotspot_ip_wins() {
        assert_eq!(score_interface("Local Area Connection* 12", Ipv4Addr::new(192, 168, 137, 1)), 100);
    }

    #[test]
    fn loopback_scores_zero() {
        assert_eq!(score_interface("lo", Ipv4Addr::LOCALHOST), 0);
    }

    #[test]
    fn wifi_direct_group_owner_ip_is_a_hotspot() {
        assert_eq!(score_interface("Local Area Connection* 3", Ipv4Addr::new(192, 168, 49, 1)), 100);
    }

    #[test]
    fn home_router_client_ip_is_weak() {
        assert_eq!(score_interface("wlan0", Ipv4Addr::new(192, 168, 1, 42)), 20);
    }

    #[test]
    fn apipa_address_on_hotspot_named_adapter_is_not_a_hotspot() {
        // ICS has not (yet) given the adapter its gateway address: binding here is useless.
        assert!(!is_hotspot("Local Area Connection* 12", Ipv4Addr::new(169, 254, 10, 5)));
        assert!(!is_hotspot("Microsoft Wi-Fi Direct Virtual Adapter #2", Ipv4Addr::new(169, 254, 0, 1)));
    }

    #[test]
    fn name_hint_without_gateway_address_is_not_a_hotspot() {
        assert!(!is_hotspot("Local Area Connection* 12", Ipv4Addr::new(192, 168, 1, 42)));
        assert!(!is_hotspot("hotspot0", Ipv4Addr::UNSPECIFIED));
        assert!(!is_hotspot("hotspot0", Ipv4Addr::LOCALHOST));
    }

    #[test]
    fn name_hint_with_private_gateway_address_is_a_hotspot() {
        assert!(is_hotspot("Local Area Connection* 12", Ipv4Addr::new(10, 0, 0, 1)));
        assert!(is_hotspot("Local Area Connection* 3", Ipv4Addr::new(192, 168, 137, 1)));
        assert!(is_hotspot("wlan0", Ipv4Addr::new(192, 168, 49, 1)));
    }

    #[test]
    fn gateway_address_without_name_hint_is_not_a_hotspot() {
        assert!(!is_hotspot("Ethernet", Ipv4Addr::new(192, 168, 1, 1)));
    }

    #[test]
    fn name_hint_is_a_tiebreaker_in_scoring() {
        let hinted_apipa = score_interface("Local Area Connection* 12", Ipv4Addr::new(169, 254, 10, 5));
        let plain_client = score_interface("Ethernet", Ipv4Addr::new(192, 168, 1, 42));
        let plain_gateway = score_interface("Ethernet", Ipv4Addr::new(192, 168, 1, 1));
        assert!(hinted_apipa > plain_client);
        assert!(hinted_apipa > plain_gateway);
        assert!(hinted_apipa < 60, "a name hint alone must not reach the hotspot threshold");
    }

    #[test]
    fn hotspot_candidate_prefers_gateway_then_hotspot_named_adapter() {
        let wifi = make("Wi-Fi".into(), Ipv4Addr::new(192, 168, 1, 42), None);
        let stuck = make("Local Area Connection* 12".into(), Ipv4Addr::new(169, 254, 10, 5), Some("h".into()));
        let ready = make("Local Area Connection* 12".into(), Ipv4Addr::new(192, 168, 137, 1), Some("h".into()));
        // Stuck on APIPA: still the adapter "Fix sharing" must target, but not one to bind to.
        let c = hotspot_adapter_candidate(vec![wifi.clone(), stuck.clone()]).expect("candidate");
        assert_eq!(c.ip, "169.254.10.5");
        assert!(!c.is_hotspot);
        let c = hotspot_adapter_candidate(vec![wifi.clone(), ready.clone()]).expect("candidate");
        assert_eq!(c.ip, "192.168.137.1");
        assert!(c.is_hotspot);
        assert!(hotspot_adapter_candidate(vec![wifi]).is_none());
    }
}
