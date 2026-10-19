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

1. Turn on your OS hotspot (Windows *Mobile hotspot*, macOS *Internet Sharing*, Linux NetworkManager *Hotspot*).
2. Launch the app. It scans local adapters and picks the hotspot gateway automatically
   (`192.168.137.1` on Windows, `192.168.2.1` on macOS, `10.42.0.1` on Linux). You can override it.
3. Add files/folders with the buttons or by dragging them onto the window.
4. Flip the **Server** switch. The HTTP server binds to the hotspot IP on port `8080` (falls back to `8081`).
5. On the phone, scan the QR code or open `http://192.168.137.1:8080` in any browser.

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
├── pool.rs      in-memory pool, /<name> → host path mapping, dedupe
├── vpath.rs     virtual path resolution + listings (no `..` escapes)
├── http.rs      axum server: page, /api/list, /dl (Range), telemetry, events
└── lib.rs       Tauri commands: start_server, stop_server, get_server_status,
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
VITE_TAURI_MOCK=1 pnpm dev                        # UI in a normal browser with a mocked Tauri bridge
pnpm tauri build                       # production bundles in src-tauri/target/release/bundle
```

The release profile is tuned for size (`opt-level = "z"`, `lto`, `codegen-units = 1`, `panic = "abort"`, `strip`).

## Firewall note

Windows may prompt to allow the app through the firewall the first time the server starts. Allow it on **private** networks. Only TCP port `8080` (or `8081`) is used.

## License

MIT
