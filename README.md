# Hotspot Pool

Share any files or folders on your laptop with phones connected to your **Wi-Fi Mobile Hotspot**, through an embedded HTTP server and a mobile-friendly web page. No router, no cloud, no app on the phone: scan the QR code and download.

Built with **Tauri v2 + Rust** (backend) and **React + Tailwind CSS v4 + shadcn-style components** (frontend).

```
┌───────────────────────────────────────────────────────────────────────────┐
│ ● Hotspot Pool                          ● SHARING  [toggle]      – ▢ ✕   │
├──────────┬────────────────────────────────────────────────────────────────┤
│ Share    │  Sharing is on                              [ Stop sharing ]   │
│ Pool   5 │  Phones on ABD-Laptop can type abdu.link                      │
│ Activity │  LINK FOR THE PHONE   http://abdu.link            ⧉  Open     │
│ Settings │  ┌ 1 · Join the Wi-Fi ──────┐ ┌ 2 · Open the page ──────────┐ │
│          │  │ [QR]  Network  ABD-Laptop│ │ [QR]  Type this  abdu.link  │ │
│ ● Sharing│  │       Password ••••••••  │ │                             │ │
├──────────┴────────────────────────────────────────────────────────────────┤
│ HTTP 80 · Pool 5 items · 15.9 GB                 16:35 DOWNLOAD a.mp4    │
└───────────────────────────────────────────────────────────────────────────┘
```

## How it works

**Windows (one click):**

1. Add files or folders with the buttons or by dragging them onto the window.
2. Flip the **Server** switch. The app turns on *Mobile hotspot* through the Windows tethering API (no admin needed), waits for the `192.168.137.1` adapter and starts the HTTP server on port `8080` (falls back to `8081`).
3. On the phone, scan QR **1** with the camera to join the hotspot Wi-Fi, then QR **2** to open the page, or just type the share name, e.g. `abdu.link`. No IP addresses.
4. Flip the switch off and the app stops sharing and turns the hotspot off again (if it was the one that turned it on). Untick *Manage the hotspot with the Server switch* to leave the hotspot alone.

The pool is saved to `pool.json` in the app's config folder and restored on the next launch (items whose files disappeared are dropped with a note in the log).

The hotspot name and password are shown in Settings and can be edited there.

**Share name (`<name>.<ending>`).** Pick the label and ending in the sidebar (default `share.link`). The ending must be a **real top-level domain** (`link`, `site`, `net`, …): phone browsers only open typed text as an address when it ends in one they know, otherwise they search the web for it. On a Windows hotspot the ICS DNS proxy resolves through the laptop's own resolver, so the app adds `192.168.137.1 <name>.<ending>` to the Windows hosts file (one UAC prompt, only when the name changes) and verifies with a real DNS query to the gateway before showing the name as the main link. The server is plain HTTP on port 80 (8080/8081 fallback); there is deliberately no HTTPS, since a self-signed certificate would make every phone show a warning.

**macOS / Linux:** hotspot control is manual. Turn on *Internet Sharing* (macOS) or a NetworkManager *Hotspot* (Linux), then flip the switch; the app finds the gateway (`192.168.2.1` / `10.42.0.1`) and starts the server. The *Open hotspot settings* button jumps to the right settings pane.

The phone sees one unified listing regardless of which drive each item lives on. Folders can be browsed and **any single file inside can be downloaded**; images, video, audio and PDFs can be previewed in place (video streams with HTTP range support). The share is **read-only**.

## Web page (what the phone sees)

* Dark, touch-first list: folders first, type-coloured icons, size and age per file.
* Breadcrumb navigation, in-folder search, sort by name / size / newest.
* Tap a file to download it; tap the eye to preview media; folders open in place.
* **Download manager:** each file shows a progress bar with speed and time left; a **Downloads** tab lists everything with pause / resume (HTTP ranges), cancel, "Pause all" and "Clear finished". Downloads run inside the page (plain `http://` pages cannot use service workers), chunks are stored in IndexedDB on the phone, and a finished file is handed to the browser with one tap on **Save**.
* **Upload tab:** phones can send files to the laptop with per-file progress. Files land in a quarantine **inbox** on the laptop and are shared only after approval.
* Auto-refreshes every 15 s and whenever the tab becomes visible.

## Uploads and security

* **Quarantine first.** Uploads are written to `<app data>/inbox/` (changeable in Settings) and never served until the laptop user approves them in the **Inbox** page. Approve moves the file into a shared `Uploads` folder in the pool; Reject deletes it. Executables get a warning badge.
* **PIN.** Optional upload PIN; 5 wrong attempts lock that IP for a minute. Uploads can be switched off entirely.
* **Limits.** Per-file maximum and total inbox quota are checked against `Content-Length` before any data is read, and again while streaming; oversize transfers are cut off and the partial file deleted.
* **Names.** The phone's file name is only used for display after sanitising (no path separators, control characters, Windows reserved names, max 150 chars); on disk each upload is stored under a random id. A SHA-256 of the content is recorded and shown.
* **Streaming.** Uploads stream to a temp file and are renamed on completion; at most 3 run concurrently; stale partials older than a day are swept on start.

API: `GET /api/upload-info`, `POST /api/upload` with raw body and headers `X-File-Name` (percent-encoded), `X-Device-Name`, `X-Upload-Pin`.

HTTP API used by the page: `GET /api/list?path=/Videos/sub` (JSON) and `GET /dl/<path>` (download, `?inline=1` for preview, supports `Range`).

## Architecture

```
src-tauri/src
├── network.rs   interface scan + hotspot scoring (if-addrs)
├── hotspot.rs   Windows Mobile hotspot on/off + credentials (WinRT tethering API)
├── pool.rs      in-memory pool, /<name> → host path mapping, dedupe
├── vpath.rs     virtual path resolution + listings (no `..` escapes)
├── http.rs      axum server: page, /api/list, /dl (Range), /api/upload, telemetry, events
├── uploads.rs   quarantine inbox: sanitised names, approve/reject, quota
└── lib.rs       Tauri commands: auto_share, stop_share, start_server, stop_server,
                 get_server_status, get_hotspot_info, set_hotspot, configure_hotspot,
                 add_path_to_pool, remove_path_from_pool, get_pool,
                 get_network_interfaces, get_hotspot_interface
src-tauri/web/index.html   the phone page (embedded into the binary)
src
├── hooks/       useServer (status/stats/log listeners), usePool, useDragDrop
├── components/  TopBar, Nav (Share · Pool · Activity · Settings), PoolManager, StatusBar
├── components/tabs/  ShareTab (hero, link, QR steps), InboxTab (review uploads), ActivityTab, SettingsTab
└── lib/tauri.ts typed invoke/listen wrappers
```

Events emitted to the UI: `server_log`, `server_stats` (every 500 ms), `server_status`, `pool_changed`.

## Development

Prereqs: Rust stable, Node 22+, pnpm, and the [Tauri v2 system deps](https://v2.tauri.app/start/prerequisites/).

```bash
pnpm install
pnpm tauri dev                         # run with hot reload
cargo test --manifest-path src-tauri/Cargo.toml   # unit tests + end-to-end curl against the real HTTP server
cargo check --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-gnu   # type-check the Windows-only code (needs mingw-w64 binutils)
VITE_TAURI_MOCK=1 pnpm dev                        # UI in a normal browser with a mocked Tauri bridge
pnpm tauri build                       # production bundles in src-tauri/target/release/bundle
```

The release profile is tuned for size (`opt-level = "z"`, `lto`, `codegen-units = 1`, `panic = "abort"`, `strip`).

## Firewall note

Windows may prompt to allow the app through the firewall the first time the server starts. Allow it on **private** networks. Only TCP port `80` (or `8080`/`8081`) is used.

## License

MIT
