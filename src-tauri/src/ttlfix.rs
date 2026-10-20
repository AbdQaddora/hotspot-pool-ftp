#![allow(dead_code)] // wired to the WinDivert loop once the driver can be bundled (CI only)
//! Resetting the IPv4 TTL on forwarded packets.
//!
//! Some networks (a common MikroTik anti-tethering rule) force the TTL of the
//! traffic they hand the laptop down to 1, so the moment the laptop forwards a
//! packet to a phone the TTL reaches 0 and the packet is dropped: the laptop
//! has internet, the phones do not. Raising the TTL on those packets before
//! the laptop forwards them restores it. Only the IPv4 header checksum has to
//! be recomputed, since the TCP/UDP checksum does not cover the TTL field.

/// Standard IPv4 header checksum over `header` (the 20+ byte IPv4 header, with
/// its own checksum field treated as zero). Returns the value to store.
pub fn ipv4_header_checksum(header: &[u8]) -> u16 {
    let ihl = ((header[0] & 0x0f) as usize) * 4;
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 1 < ihl {
        // The checksum field itself (bytes 10..12) is skipped by treating it as zero.
        let word = if i == 10 { 0 } else { ((header[i] as u32) << 8) | header[i + 1] as u32 };
        sum += word;
        i += 2;
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

/// If `packet` is an IPv4 packet whose TTL is below `new_ttl`, raise it to
/// `new_ttl` and fix the header checksum. Returns true when it changed the
/// packet. Non-IPv4 or already-high-enough packets are left untouched.
pub fn raise_ttl(packet: &mut [u8], new_ttl: u8) -> bool {
    if packet.len() < 20 || (packet[0] >> 4) != 4 {
        return false;
    }
    let ihl = ((packet[0] & 0x0f) as usize) * 4;
    if ihl < 20 || packet.len() < ihl {
        return false;
    }
    if packet[8] >= new_ttl {
        return false;
    }
    packet[8] = new_ttl;
    let ck = ipv4_header_checksum(&packet[..ihl]);
    packet[10] = (ck >> 8) as u8;
    packet[11] = (ck & 0xff) as u8;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    // A real IPv4 header: 45 00 00 3c ... src 192.168.0.1 dst 192.168.0.2, TTL at [8].
    fn sample() -> Vec<u8> {
        vec![
            0x45, 0x00, 0x00, 0x3c, 0x1c, 0x46, 0x40, 0x00, 0x01, 0x06, 0x00, 0x00, 0xc0, 0xa8, 0x00, 0x01, 0xc0, 0xa8,
            0x00, 0xc7, 0x00, 0x50, 0x00, 0x16,
        ]
    }

    #[test]
    fn checksum_matches_known_header() {
        // Standard worked example (RFC-style): header with TTL 0x40 checksums to 0xb861.
        let hdr = vec![0x45, 0x00, 0x00, 0x73, 0x00, 0x00, 0x40, 0x00, 0x40, 0x11, 0x00, 0x00, 0xc0, 0xa8, 0x00, 0x01, 0xc0, 0xa8, 0x00, 0xc7];
        assert_eq!(ipv4_header_checksum(&hdr), 0xb861);
    }

    #[test]
    fn checksum_is_valid_after_recompute() {
        let mut p = sample();
        let ck = ipv4_header_checksum(&p[..20]);
        p[10] = (ck >> 8) as u8;
        p[11] = (ck & 0xff) as u8;
        // A correct checksum makes the full-header sum fold to 0xffff.
        let mut sum: u32 = 0;
        let mut i = 0;
        while i < 20 {
            sum += ((p[i] as u32) << 8) | p[i + 1] as u32;
            i += 2;
        }
        while (sum >> 16) != 0 {
            sum = (sum & 0xffff) + (sum >> 16);
        }
        assert_eq!(sum as u16, 0xffff);
    }

    #[test]
    fn raise_ttl_sets_and_fixes_checksum() {
        let mut p = sample();
        p[8] = 1; // simulate the network forcing TTL to 1
        let ck0 = [p[10], p[11]];
        assert!(raise_ttl(&mut p, 128));
        assert_eq!(p[8], 128);
        assert_ne!([p[10], p[11]], ck0);
        // Recomputed checksum must be valid.
        assert_eq!(ipv4_header_checksum(&p[..20]), ((p[10] as u16) << 8) | p[11] as u16);
    }

    #[test]
    fn raise_ttl_leaves_high_ttl_and_non_ipv4() {
        let mut p = sample();
        p[8] = 200;
        assert!(!raise_ttl(&mut p, 128));
        assert_eq!(p[8], 200);
        let mut v6 = vec![0x60; 40];
        assert!(!raise_ttl(&mut v6, 128));
        let mut tiny = vec![0x45, 0x00];
        assert!(!raise_ttl(&mut tiny, 128));
    }
}
