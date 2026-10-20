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

fn score_interface(name: &str, ip: Ipv4Addr) -> u8 {
    if ip.is_loopback() {
        return 0;
    }
    if KNOWN_HOTSPOT_IPS.contains(&ip) {
        return 100;
    }
    let lname = name.to_ascii_lowercase();
    let name_hint = lname.contains("hotspot")
        || lname.contains("bridge100")
        || lname.starts_with("ap")
        || lname.contains("local area connection*")
        || lname.contains("wi-fi direct")
        || lname.contains("microsoft wi-fi direct");
    let is_gateway_like = ip.is_private() && ip.octets()[3] == 1;
    match (name_hint, is_gateway_like) {
        (true, true) => 90,
        (true, false) => 60,
        (false, true) => 40,
        (false, false) if ip.is_private() => 20,
        _ => 10,
    }
}

/// Enumerate IPv4 interfaces, highest hotspot confidence first.
pub fn list_interfaces() -> Vec<NetInterface> {
    let mut out: Vec<NetInterface> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|iface| match iface.addr {
            if_addrs::IfAddr::V4(v4) => {
                let score = score_interface(&iface.name, v4.ip);
                #[cfg(windows)]
                let adapter_id = crate::ics::norm_guid(&iface.adapter_name);
                #[cfg(not(windows))]
                let adapter_id = None;
                Some(NetInterface {
                    name: iface.name,
                    ip: v4.ip.to_string(),
                    is_loopback: v4.ip.is_loopback(),
                    is_hotspot: score >= 60,
                    score,
                    adapter_id,
                })
            }
            _ => None,
        })
        .collect();
    out.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.name.cmp(&b.name)));
    out
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
}
