# Hotspot Pool FTP

Share any files or folders on your laptop with phones connected to your **Wi-Fi Mobile Hotspot**, through an embedded HTTP server and a mobile-friendly web page. No router, no cloud, no app on the phone: scan the QR code and download.

Built with **Tauri v2 + Rust** (backend) and **React + Tailwind CSS v4 + shadcn-style components** (frontend).

```
┌───────────────────────────────────────────────────────────────────────────┐
│ ● Hotspot Pool FTP                        ● SERVER ON  [toggle]   – ▢ ✕  │
├──────────────┬────────────────────────────────────────────────────────────┤
│ HOTSPOT      │ Shared Pool            [+ Add Folders] [+ Add Files]       │
│ 192.168.137.1│ ─────────────────────────────────────────────────────────  │
│ :8080        │ ▶ Videos        D:\Media\Videos            12.4 GB  ⧉ ⌂ ✕ │
│ [QR code]    │ ▪ report.pdf    C:\Users\me\Docs\report.pdf 1.2 MB  ⧉ ⌂ ✕ │
│ 2 connected  │                                                            │
│ 11.3 MB/s ▁▃▅│                 (drop files anywhere)                      │
├──────────────┴────────────────────────────────────────────────────────────┤
│ HTTP 8080 · Pool 2 items · Total 12.4 GB        12:01:07 DOWNLOAD a.mp4   │
└───────────────────────────────────────────────────────────────────────────┘
```

## How it works

**Windows (one click):**

1. Add files or folders with the buttons or by dragging them onto the window.
2. Flip the **Server** switch. The app turns on *Mobile hotspot* through the Windows tethering API (no admin needed), waits for the `192.168.137.1` adapter and starts the HTTP server on port `8080` (falls back to `8081`).
3. On the phone, scan QR **1** with the camera to join the hotspot Wi-Fi, then QR **2** to open the page. No typing.
4. Flip the switch off and the app stops sharing and turns the hotspot off again (if it was the one that turned it on). Untick *Manage the hotspot with the Server switch* to leave the hotspot alone.

The hotspot name and password are shown in the sidebar and can be edited there. Windows only starts a hotspot when the laptop has a connection to share (Wi-Fi or Ethernet); otherwise the app explains what to do.

**macOS / Linux:** hotspot control is manual. Turn on *Internet Sharing* (macOS) or a NetworkManager *Hotspot* (Linux), then flip the switch; the app finds the gateway (`192.168.2.1` / `10.42.0.1`) and starts the server. The *Open hotspot settings* button jumps to the right settings pane.

The phone sees one unified listing regardless of which drive each item lives on. Folders can be browsed and **any single file inside can be downloaded**; images, video, audio and PDFs can be previewed in place (video streams with HTTP range support). The share is **read-only**.

## Web page (what the phone sees)

* Dark, touch-first list: folders first, type-coloured icons, size and age per file.
* Breadcrumb navigation, in-folder search, sort by name / size / newest.
* Tap a file to download it; tap the eye to preview media; folders open in place.
* Auto-refreshes every 15 s and whenever the tab becomes visible.

HTTP API used by the page: `GET /api/list?path=/Videos/sub` (JSON) and `GET /dl/<path>` (download, `?inline=1` for preview, supports `Range`).

## Architecture

```
src-tauri/src
├── network.rs   interface scan + hotspot scoring (if-addrs)
├── hotspot.rs   Windows Mobile hotspot on/off + credentials (WinRT tethering API)
├── pool.rs      in-memory pool, /<name> → host path mapping, dedupe
├── vpath.rs     virtual path resolution + listings (no `..` escapes)
├── http.rs      axum server: page, /api/list, /dl (Range), telemetry, events
└── lib.rs       Tauri commands: auto_share, stop_share, start_server, stop_server,
                 get_server_status, get_hotspot_info, set_hotspot, configure_hotspot,
                 add_path_to_pool, remove_path_from_pool, get_pool,
                 get_network_interfaces, get_hotspot_interface
src-tauri/web/index.html   the phone page (embedded into the binary)
src
├── hooks/       useServer (status/stats/log listeners), usePool, useDragDrop
├── components/  TopBar, Sidebar (adapter, QR, metrics), PoolManager, StatusBar
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

Windows may prompt to allow the app through the firewall the first time the server starts. Allow it on **private** networks. Only TCP port `8080` (or `8081`) is used.

## License

MIT
