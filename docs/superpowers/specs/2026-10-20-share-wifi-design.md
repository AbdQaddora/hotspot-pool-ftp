# Share Wi-Fi — design

**Intent.** Give phones on the laptop's hotspot the laptop's internet, the way
MyPublicWiFi does in its "Internet Connection Sharing (ICS)" mode. The user's
ISP (a MikroTik captive portal with voucher login) only admits the laptop;
phones behind the laptop's NAT ride on that login. Today the hotspot comes up
but phones get "connected, no internet": the app flips Windows Mobile Hotspot
and trusts Windows to wire ICS from the right upstream, and that silently fails.

**What it copies from MyPublicWiFi.** Hotspot on/off on its own (no file
server needed); choose the internet connection to share; Windows ICS set
explicitly between that connection (public) and the hotspot adapter (private);
connected device list; a sign-in-needed state for captive portals.

**Non-goals.** Router Mode (own DHCP/NAT, needs a kernel driver), bandwidth
limits, user authentication/captive portal, URL logging, ad blocking.

## Behaviour

New tab **Share Wi-Fi** between Share and Pool.

- **Hotspot switch**: turns Windows Mobile Hotspot on/off. Independent of file
  sharing. Shows SSID, password (hidden/show/copy), Wi-Fi join QR.
- **Internet to share**: dropdown of connected profiles ("Automatic" = the
  connection Windows considers the internet one, plus each profile with its
  kind and connectivity). Persisted in settings as `upstreamAdapter`
  (adapter GUID, null = automatic). When set, the hotspot is created from that
  profile.
- **Health line** (one of): `sharing` "Devices get the laptop's internet";
  `not_shared` "Windows is not sharing the internet to the hotspot";
  `wrong_upstream` "Windows is sharing <X> instead of <Y>"; `signin` "Sign in
  on the laptop first"; `no_internet` "The laptop has no internet to share";
  `hotspot_off` "Turn on the hotspot"; `unknown` "Could not read the sharing
  state". `not_shared`, `wrong_upstream` and `unknown` show **Fix sharing
  (admin)**; `signin` shows **Open sign-in page**.
- **Fix sharing (admin)**: one UAC prompt; disables ICS everywhere, enables
  public on the upstream, private on the hotspot adapter; re-reads state.
- **Devices**: list of tethering clients (name or "Unknown device", MAC),
  refreshed every 5 s while the tab is open.

## Mechanics (Windows)

- Upstreams: WinRT `NetworkInformation::GetConnectionProfiles()`; id = adapter
  GUID (lowercase, no braces); kind from `IanaInterfaceType` (6 ethernet, 71
  wifi, 243/244 cellular, else other); connectivity from
  `GetNetworkConnectivityLevel` (InternetAccess→internet,
  ConstrainedInternetAccess→signin, LocalAccess→local, None→none);
  `is_default` when it equals `GetInternetConnectionProfile()`.
- ICS state: hidden non-elevated PowerShell using `HNetCfg.HNetShare`, JSON
  out (`id`, `name`, `connected`, `enabled`, `type`). If it fails, state is
  unavailable.
- ICS write: elevated PowerShell (same UAC pattern as the hosts file),
  GUIDs validated before interpolation.
- Hotspot adapter: the interface scan already finds 192.168.137.1; its adapter
  GUID (from `if_addrs::Interface::adapter_name`) is the private side.
- Clients: `NetworkOperatorTetheringManager::GetTetheringClients()`.
- Sign-in page: open `http://www.msftconnecttest.com/redirect`.
- Non-Windows: the tab says hotspot control is manual; no ICS.
