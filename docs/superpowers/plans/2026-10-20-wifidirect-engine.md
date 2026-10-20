# Wi-Fi Direct Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give Hotspot Pool a second hotspot engine that creates its own Wi-Fi Direct access point and shares internet to it through classic ICS, exactly as MyPublicWiFi's ICS mode does, so phones get the laptop's internet.

**Architecture:** New `wfd.rs` wraps the WinRT publisher/listener in a process-wide state (publisher, listener, accepted devices). `hotspot.rs` dispatches on the engine for `set_enabled`/`info`/`clients`. `lib.rs` runs the start sequence (AP → adapter → ICS → wait for 192.168.137.1) with log lines. Frontend adds the engine choice.

**Tech Stack:** `windows` crate features Devices_WiFiDirect, Devices_Enumeration, Security_Credentials; hidden/elevated PowerShell from `elevate.rs`; existing `ics.rs`.

**Spec:** `docs/superpowers/specs/2026-10-20-wifidirect-engine-design.md`

## Global Constraints
- Linux `cargo test` green; `cargo check --target x86_64-pc-windows-gnu` clean; no new crates.
- Engine strings exactly `wifidirect` | `mobile`; unknown values load as `wifidirect`.

## Review Focus
- AP `Aborted` with `ResourceInUse` (Mobile Hotspot still on, or MyPublicWiFi running) → clear error, then automatic Mobile Hotspot fallback (Task 3 wiring; Task 2 `abort_reason` test).
- PowerShell adapter listing returns several "Up" Wi-Fi Direct adapters → pick the one not already holding 192.168.137.1? No: pick the first; log all names (Task 2 `pick_go_adapter` test with two entries).
- ICS already correct → no UAC prompt (Task 3 uses `assess` on a fresh sharing state before applying).
- Adapter never gets 192.168.137.1 within 20 s → error telling the user to click Fix sharing (Task 3).
- Settings from older versions without `hotspotEngine` → `wifidirect` (Task 1 test).

---

### Task 1: Engine setting
**Files:** `src-tauri/src/settings.rs`, `src/lib/tauri.ts`, `src/hooks/useServer.ts`, `src/dev/tauri-mock.ts`
- `Settings.hotspot_engine: String` (default `"wifidirect"`), `Settings::validate_engine(&str) -> Result<String,String>` accepting only the two values; `set_settings` validates.
- [ ] Test `engine_defaults_and_validation`: default is `wifidirect`; `{"name":"abdu"}` loads as `wifidirect`; `validate_engine("Mobile")=="mobile"`; `validate_engine("x")` is Err. Run → fail → implement → pass → commit `feat: hotspot engine setting`.

### Task 2: `wfd.rs` — Wi-Fi Direct AP
**Files:** create `src-tauri/src/wfd.rs`; `src-tauri/Cargo.toml` features; `src-tauri/src/lib.rs` `mod wfd;`
**Interfaces (all platforms):** `pub struct ApInfo { running: bool, ssid: Option<String>, passphrase: Option<String>, clients: Vec<HotspotClient>, error: Option<String> }`; `pub fn start(ssid:&str, passphrase:&str) -> Result<ApInfo,String>`; `pub fn stop()`; `pub fn info() -> ApInfo`; `pub fn abort_reason(error_code: i32) -> &'static str` (1 → "Wi-Fi is turned off", 2 → "Another hotspot program is using the Wi-Fi adapter (turn off Mobile Hotspot and MyPublicWiFi)", other → "Windows could not start the access point"); `pub fn go_adapter_script() -> &'static str`; `pub fn pick_go_adapter(json:&str) -> Option<(String /*name*/, String /*guid norm*/)>`.
- [ ] Tests: `abort_reasons`; `pick_go_adapter_parses_array_object_and_empty` (array of `{Name, InterfaceGuid, Status}` → first; bare object; "" → None; garbage → None).
- [ ] Windows impl per the Microsoft sample: publisher + `SetIsAutonomousGroupOwnerEnabled(true)`, `LegacySettings().SetIsEnabled(true)`, `SetSsid`, `Passphrase()?.SetPassword`, `StatusChanged` handler writes status/error into `Arc<Mutex<_>>`, `ConnectionRequested` handler spawns a thread doing `WiFiDirectDevice::FromIdAsync(&id)?.join()`, stores the device keyed by id with name (`DeviceInformation().Name()`) and ip (first `GetConnectionEndpointPairs()` RemoteHostName), registers `ConnectionStatusChanged` to drop it on `Disconnected`; `Start()`; wait ≤ 10 s for Started/Aborted. `stop()` calls `Stop()`, closes devices, clears state. Non-Windows: `start` → Err("Not supported on this OS"), `info` → not running.
- [ ] `cargo test`, Windows `cargo check`, commit `feat(wfd): Wi-Fi Direct legacy access point`.

### Task 3: Engine dispatch and start sequence
**Files:** `src-tauri/src/hotspot.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/ics.rs`
- `hotspot::info()`: if `wfd::info().running` → `HotspotInfo { supported: true, state: "on", ssid, passphrase, client_count, wifi_qr }`; else Mobile Hotspot as today. `hotspot::clients()`: WFD clients when running, else tethering clients.
- `hotspot::set_enabled(on, preferred)` unchanged (Mobile Hotspot); new `hotspot::mobile_off_if_on()`.
- `lib.rs`: `async fn hotspot_on(app, engine_state) -> Result<HotspotInfo,String>` implementing spec steps 1–5 with `emit_log` lines ("Starting the access point…", "Access point \"X\" is on", "Sharing <upstream> with the access point (Windows will ask for permission)…", "Waiting for Windows to give the access point an address…"); on AP start failure with engine `wifidirect`: log warn and fall back to `hotspot::set_enabled(true, preferred)`. `hotspot_off(app)`: `wfd::stop()` if running else Mobile Hotspot off. `set_hotspot`, `auto_share`, `stop_share` use these. `ics::wait_for_gateway(adapter_id, timeout) -> Option<NetInterface>` (pure loop over `list_interfaces`).
- [ ] Test `ics::gateway_wait_predicate` on a pure `fn is_gateway(i:&NetInterface, adapter_id:&str) -> bool` (adapter matches and ip == 192.168.137.1).
- [ ] `cargo test`, Windows `cargo check`, commit `feat: Wi-Fi Direct engine start sequence with ICS`.

### Task 4: Frontend
**Files:** `src/components/tabs/WifiTab.tsx`, `src/components/tabs/SettingsTab.tsx`, `src/lib/tauri.ts`, `src/dev/tauri-mock.ts`, README
- Engine selector (select) in Share Wi-Fi "Internet to share" card: "Access point + Windows sharing (recommended)" / "Windows Mobile Hotspot", saving `hotspotEngine`; health copy unchanged. Settings tab shows the same control under Mobile hotspot card.
- [ ] `tsc`, mock build, screenshot `?autostart&tab=wifi`; commit `feat(ui): hotspot engine choice`.

### Task 5: Release 0.10.0
- README section "Hotspot engine"; versions; full checks; merge to main; push; CI; tag; release with notes.
