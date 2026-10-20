# Wi-Fi Direct access point engine (MyPublicWiFi's way) — design

**Finding (2026-10-20).** MyPublicWiFi does not use Windows Mobile Hotspot. Its
`WiFiDirectAP.dll` creates the access point with the WinRT
`WiFiDirectAdvertisementPublisher` in legacy mode (autonomous group owner +
`LegacySettings` SSID/passphrase, `WiFiDirectConnectionListener` accepting every
`ConnectionRequested` via `WiFiDirectDevice::FromIdAsync`). In its "ICS" mode it
then enables classic Internet Connection Sharing (`HNetCfg.HNetShare`, the same
COM object we use) from the upstream to that virtual adapter. Its "Router Mode"
does NAT itself on the Windows Packet Filter driver (`ndisrd.sys`, $3,000
redistribution licence) — out of scope.

Why ours failed: modern Mobile Hotspot forwards through WinNAT and is mutually
exclusive with classic ICS; enabling ICS on top of it fails (0x80040201) or
breaks the hotspot. With a Wi-Fi Direct AP of our own there is no WinNAT
instance, so ICS works — and the ICS DNS proxy reads the hosts file, so the
share name keeps working.

## Behaviour

- New setting `hotspotEngine`: `"wifidirect"` (default, "Access point + Windows
  sharing, the MyPublicWiFi way") or `"mobile"` ("Windows Mobile Hotspot").
  Shown in the Share Wi-Fi tab under the hotspot switch and in Settings.
- With `wifidirect`, turning the hotspot on (Share switch or Share Wi-Fi tab):
  1. If Mobile Hotspot is on, turn it off (Windows forbids both at once).
  2. Start the AP with the Mobile Hotspot's configured SSID/passphrase (so the
     QR code and the user's saved network stay the same). Wait ≤ 10 s for
     `Started`; `Aborted` → error naming the reason (radio off / resource in
     use / other).
  3. Find the group-owner adapter: `Get-NetAdapter -IncludeHidden` where
     InterfaceDescription starts with "Microsoft Wi-Fi Direct Virtual Adapter"
     and Status is Up (hidden PowerShell, JSON).
  4. Make sure ICS is public = effective upstream, private = that adapter;
     if not, apply it (one UAC prompt; stale-flag cleanup already in place).
  5. Wait ≤ 20 s for the adapter to get 192.168.137.1 (ICS assigns it), then
     the rest of the flow is unchanged (server binds, hosts-file name).
  Turning it off stops the AP; ICS stays configured (no UAC on stop).
- Automatic fallback: if the AP fails to start, log it and use Mobile Hotspot.
- `hotspot::info()` reports the AP when it is running (state "on", SSID,
  passphrase, client count from accepted devices); `clients()` lists accepted
  devices with name and IP from `GetConnectionEndpointPairs`.
- Share Wi-Fi health works unchanged; with the AP engine the expected steady
  state is `sharing`.

## Non-goals
Router Mode / own NAT; Hosted Network (deprecated API); hiding the SSID.
