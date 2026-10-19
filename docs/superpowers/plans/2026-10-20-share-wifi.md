# Share Wi-Fi Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Share Wi-Fi tab that gives hotspot devices the laptop's internet by choosing the upstream connection and setting Windows Internet Connection Sharing explicitly, with a health line, fix button, sign-in detection and device list.

**Architecture:** New Rust module `ics.rs` (upstream list via WinRT, ICS state via hidden PowerShell JSON, ICS write via elevated PowerShell, pure `assess()` health rule). `hotspot.rs` gains a preferred-upstream profile and a client list. One new Tauri command returns a `WifiShareStatus` snapshot; one applies sharing. Frontend: `WifiTab.tsx` + `useWifiShare.ts` hook polling the snapshot.

**Tech Stack:** Tauri v2, Rust (`windows` crate: Networking_Connectivity, Networking_NetworkOperators), PowerShell `HNetCfg.HNetShare`, React/Tailwind v4, Vite mock for screenshots.

**Spec:** `docs/superpowers/specs/2026-10-20-share-wifi-design.md`

## Global Constraints

- Windows-only code lives behind `#[cfg(windows)]`; `cargo test` runs on Linux and must pass; `cargo check --target x86_64-pc-windows-gnu` must pass.
- GUIDs are stored lowercase without braces; the only braces/uppercase form is inside generated PowerShell.
- No new crates. No `cd` in parallel shell calls.
- Health strings exactly: `sharing`, `not_shared`, `wrong_upstream`, `signin`, `no_internet`, `hotspot_off`, `unknown`.

## Review Focus

- PowerShell output with zero connections or non-JSON (COM refused) → `SharingState.available = false`, never a panic. (Task 2 test `parse_rejects_garbage`.)
- A malformed GUID from settings must never reach the elevated script. (Task 2 test `script_rejects_bad_guid`.)
- Selected upstream no longer connected → fall back to automatic, health computed from the fallback. (Task 4 test `assess_wrong_upstream_vs_not_shared` covers the rule; runtime fallback in `effective_upstream`.)
- Hotspot adapter not found (Windows hasn't created it yet) → health `unknown` instead of `wrong_upstream`, and the fix command returns an error naming the hotspot. (Task 2 `assess_private_unknown`.)
- Non-Windows: status returns `available=false`, empty upstreams, and the tab renders without errors (mock build + screenshot in Task 6).

---

### Task 1: Elevation helper shared with the hosts-file code

**Files:**
- Create: `src-tauri/src/elevate.rs`
- Modify: `src-tauri/src/names.rs` (replace inline PowerShell launch), `src-tauri/src/lib.rs` (`mod elevate;`)

**Interfaces:**
- Produces: `pub fn run_elevated(script_name: &str, body: &str) -> Result<(), String>` (Windows: writes `%TEMP%\hotspot-pool\<script_name>`, launches hidden elevated PowerShell, waits; maps UAC cancel to the existing "prompt was cancelled" message prefix; non-Windows: `Err("Not supported on this OS")`).
- Produces: `pub fn run_hidden(body: &str) -> Result<String, String>` (Windows: `powershell -NoProfile -NonInteractive -Command`, CREATE_NO_WINDOW, returns stdout; non-Windows: Err).
- Produces: `pub fn ps_quote(s: &str) -> String` (single-quote PowerShell literal, `'` doubled).

- [ ] **Step 1: Test** in `elevate.rs`: `ps_quote_doubles_quotes`: `ps_quote("a'b") == "'a''b'"`.
- [ ] **Step 2: Run** `cargo test --manifest-path src-tauri/Cargo.toml ps_quote` → fails (module missing).
- [ ] **Step 3: Implement** `elevate.rs`; move the `Start-Process -Verb RunAs` launcher out of `names::ensure_hosts_entry` so it calls `elevate::run_elevated("apply-hosts.ps1", script)` and keeps its own hosts verification afterwards.
- [ ] **Step 4: Run** `cargo test --manifest-path src-tauri/Cargo.toml` and `cargo check --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-gnu` → pass.
- [ ] **Step 5: Commit** `refactor: shared elevated/hidden PowerShell runner`.

### Task 2: `ics.rs` pure core (parsing, scripts, health rule)

**Files:**
- Create: `src-tauri/src/ics.rs`
- Modify: `src-tauri/src/lib.rs` (`mod ics;`)

**Interfaces:**
- Produces types (all `Serialize`, camelCase): `Upstream { id, name, kind: &'static str, connectivity: &'static str, is_default: bool }`, `SharingConnection { id, name, connected: bool, sharing: &'static str /* public|private|none */ }`, `SharingState { available: bool, connections: Vec<SharingConnection>, public_id: Option<String>, private_id: Option<String>, error: Option<String> }`.
- `pub fn norm_guid(s: &str) -> Option<String>` → lowercase, braces stripped, must be 36 chars `8-4-4-4-12` hex.
- `pub fn parse_sharing_json(json: &str) -> SharingState` (input: array of `{id,name,connected,enabled,type}` where `type` 0 public / 1 private).
- `pub fn read_script() -> &'static str` and `pub fn apply_script(public: &str, private: &str) -> Result<String, String>` (both ids via `norm_guid`, else Err).
- `pub fn assess(hotspot_on: bool, upstream: Option<&Upstream>, sharing: &SharingState, hotspot_adapter: Option<&str>) -> &'static str`.

- [ ] **Step 1: Tests** (`#[cfg(test)]` in `ics.rs`):
  - `norm_guid_forms`: `norm_guid("{ABCDEF01-1234-5678-9ABC-DEF012345678}") == Some("abcdef01-1234-5678-9abc-def012345678")`; `norm_guid("nope") == None`.
  - `parse_sharing_sets_public_and_private`: two entries enabled with types 0 and 1 → `public_id`/`private_id` set, `available == true`.
  - `parse_rejects_garbage`: `parse_sharing_json("Access denied")` → `available == false`, `error.is_some()`.
  - `script_rejects_bad_guid`: `apply_script("x", "y").is_err()`; good GUIDs → script contains both uppercase braced GUIDs and `EnableSharing(0)` before `EnableSharing(1)`.
  - `assess_matrix`: hotspot off → `hotspot_off`; no upstream → `no_internet`; connectivity `signin` → `signin`; `local` → `no_internet`; sharing unavailable → `unknown`; public None → `not_shared`; public ≠ upstream → `wrong_upstream`; public = upstream and private = adapter → `sharing`.
  - `assess_private_unknown`: public matches, `hotspot_adapter = None` → `unknown`; public matches, private ≠ adapter → `not_shared`.
- [ ] **Step 2: Run** tests → fail (module missing).
- [ ] **Step 3: Implement** the pure functions. Read script: `HNetCfg.HNetShare`, `EnumEveryConnection`, per connection `NetConnectionProps.Invoke`, `INetSharingConfigurationForINetConnection.Invoke`, emit `ConvertTo-Json -Compress` of an array (`connected = ($p.Status -eq 2)`). Apply script: disable all, enable public, enable private.
- [ ] **Step 4: Run** tests → pass.
- [ ] **Step 5: Commit** `feat(ics): sharing state parser, scripts and health rule`.

### Task 3: Windows plumbing — upstreams, adapter GUID, clients, preferred profile

**Files:**
- Modify: `src-tauri/src/ics.rs` (Windows fns), `src-tauri/src/hotspot.rs`, `src-tauri/src/network.rs`, `src-tauri/src/settings.rs`, `src/lib/tauri.ts` (`NetInterface.adapterId`, `Settings.upstreamAdapter`)

**Interfaces:**
- `network::NetInterface` gains `pub adapter_id: Option<String>` (normalized GUID from `Interface::adapter_name` on Windows; None elsewhere).
- `settings::Settings` gains `pub upstream_adapter: Option<String>` (default None; `set_settings` normalizes via `ics::norm_guid`, rejecting invalid).
- `ics::list_upstreams() -> Vec<Upstream>` (Windows: WinRT profiles; else empty).
- `ics::sharing_state() -> SharingState` (Windows: `elevate::run_hidden(read_script())` → `parse_sharing_json`; else unavailable with error "Not supported on this OS").
- `ics::apply_sharing(public: &str, private: &str) -> Result<(), String>` (Windows: `elevate::run_elevated("apply-ics.ps1", apply_script(..)?)`).
- `hotspot::HotspotClient { mac: String, name: Option<String> }`; `hotspot::clients() -> Vec<HotspotClient>`.
- `hotspot::set_enabled(on: bool, preferred_adapter: Option<&str>)`; `pick_profile(preferred)` returns the profile whose `NetworkAdapter().NetworkAdapterId()` normalizes to `preferred`, else the existing behaviour. `ics::guid_string(&GUID) -> String` formats `data1..data4` lowercase.

- [ ] **Step 1: Test** `settings::tests::upstream_roundtrip`: `Settings { upstream_adapter: Some("abcdef01-1234-5678-9abc-def012345678".into()), .. }` round-trips through save/load; old `settings.json` without the field loads with `None` (serde default).
- [ ] **Step 2: Test** `ics::guid_string_formats_lowercase` using `windows_core::GUID` constructed from parts — Windows-only test (`#[cfg(windows)]`); on Linux write the same test against a tiny `fn guid_parts_string(d1: u32, d2: u16, d3: u16, d4: [u8; 8]) -> String` and have `guid_string` delegate to it. Assert `guid_parts_string(0xabcdef01, 0x1234, 0x5678, [0x9a,0xbc,0xde,0xf0,0x12,0x34,0x56,0x78]) == "abcdef01-1234-5678-9abc-def012345678"`.
- [ ] **Step 3: Run** → fail.
- [ ] **Step 4: Implement** all interfaces above. Update every caller of `set_enabled` in `lib.rs` to pass `settings.upstream_adapter.as_deref()`.
- [ ] **Step 5: Run** `cargo test` (Linux) and `cargo check --target x86_64-pc-windows-gnu` → pass.
- [ ] **Step 6: Commit** `feat: upstream choice, adapter ids, tethering clients`.

### Task 4: Snapshot + apply commands

**Files:**
- Modify: `src-tauri/src/lib.rs`, `src-tauri/src/ics.rs`

**Interfaces:**
- `ics::WifiShareStatus { hotspot: HotspotInfo, upstreams: Vec<Upstream>, selected_upstream: Option<String>, effective_upstream: Option<String>, sharing: SharingState, hotspot_adapter: Option<NetInterface>, clients: Vec<HotspotClient>, health: &'static str, sign_in_url: &'static str }` (`SIGN_IN_URL = "http://www.msftconnecttest.com/redirect"`).
- `pub fn effective_upstream<'a>(selected: Option<&str>, upstreams: &'a [Upstream]) -> Option<&'a Upstream>`: selected if present in list, else the `is_default` one, else first with connectivity `internet`, else first.
- Command `get_wifi_share_status() -> WifiShareStatus` (spawn_blocking; composes `hotspot::info`, `list_upstreams`, `sharing_state`, `network::detect_hotspot`, `clients`, `assess`).
- Command `apply_internet_sharing() -> Result<WifiShareStatus, String>`: needs effective upstream (else Err "Connect the laptop to the internet first.") and hotspot adapter (else Err "Turn on the hotspot first; Windows creates its adapter then."); calls `apply_sharing`, logs via `http::emit_log`, returns fresh snapshot.
- `set_hotspot` / `auto_share` use the preferred upstream from settings.

- [ ] **Step 1: Test** `ics::effective_upstream_rules` (selected present → it; selected missing → default; no default → first internet; none → first).
- [ ] **Step 2: Run** → fail. **Step 3: Implement.** **Step 4: Run** `cargo test` + Windows `cargo check` → pass.
- [ ] **Step 5: Commit** `feat: wifi share status and apply-sharing commands`.

### Task 5: Frontend — API, hook, tab, nav, mock

**Files:**
- Modify: `src/lib/tauri.ts`, `src/components/Nav.tsx`, `src/App.tsx`, `src/dev/tauri-mock.ts`, `src/hooks/useServer.ts` (expose `saveSettings` already; nothing else)
- Create: `src/hooks/useWifiShare.ts`, `src/components/tabs/WifiTab.tsx`

**Interfaces:**
- `tauri.ts`: `Upstream`, `SharingConnection`, `SharingState`, `HotspotClient`, `WifiShareStatus` mirroring Task 4 (camelCase); `api.getWifiShareStatus()`, `api.applyInternetSharing()`; `Tab` adds `"wifi"`.
- `useWifiShare({ enabled }: { enabled: boolean })` → `{ status: WifiShareStatus | null, busy, error, refresh, applySharing, dismissError }`; polls every 5000 ms while `enabled`.
- `WifiTab` props: `{ wifi: ReturnType<typeof useWifiShare>, hotspot: HotspotInfo, settings: Settings, busy: boolean, onToggleHotspot, onSaveSettings, onOpenHotspotSettings }`.
- Nav item: `{ id: "wifi", label: "Share Wi-Fi", icon: Globe, hint: "Internet for devices" }` after `share`; `?tab=wifi` supported in `App.tsx`.
- Health copy exactly as in the spec; `wrong_upstream` interpolates the public connection name and the effective upstream name.
- Mock: `get_wifi_share_status` returns a realistic snapshot whose health follows `hotspotOn` and a mutable `mockHealth` (`not_shared` until `apply_internet_sharing` is called, then `sharing`); 2 clients.

- [ ] **Step 1: Implement** types, hook, tab, nav, app wiring, mock.
- [ ] **Step 2: Verify** `pnpm exec tsc --noEmit` passes; `VITE_TAURI_MOCK=1 pnpm exec vite build` passes.
- [ ] **Step 3: Screenshot** with cached Playwright Chromium: `?tab=wifi` (hotspot off), `?autostart&tab=wifi` (hotspot on, health not_shared), after clicking Fix (health sharing). Save PNGs to the scratchpad and inspect them.
- [ ] **Step 4: Commit** `feat(ui): Share Wi-Fi tab`.

### Task 6: Docs, version, release

**Files:**
- Modify: `README.md` (new "Share Wi-Fi (internet for devices)" section after "How it works"), `package.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `src-tauri/tauri.conf.json` → `0.9.0`.

- [ ] **Step 1: Edit** docs and versions; `cargo check` to refresh `Cargo.lock`.
- [ ] **Step 2: Run** full `cargo test`, `tsc`, mock build.
- [ ] **Step 3: Commit** `chore: v0.9.0 — Share Wi-Fi`, push `main`, wait for the "Build desktop bundles" run, `git push origin v0.9.0`, `gh release create v0.9.0` with downloaded artifacts and notes.
