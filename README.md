# Hotspot Pool FTP

Share any files or folders on your laptop with phones connected to your **Wi-Fi Mobile Hotspot**, over an embedded FTP server. No router, no cloud, no app on the phone.

Built with **Tauri v2 + Rust** (backend) and **React + Tailwind CSS v4 + shadcn-style components** (frontend).

```
┌───────────────────────────────────────────────────────────────────────────┐
│ ● Hotspot Pool FTP                        ● SERVER ON  [toggle]   – ▢ ✕  │
├──────────────┬────────────────────────────────────────────────────────────┤
│ HOTSPOT      │ Shared Pool            [+ Add Folders] [+ Add Files]       │
│ 192.168.137.1│ ─────────────────────────────────────────────────────────  │
│ :2121        │ ▶ Videos        D:\Media\Videos            12.4 GB  ⧉ ⌂ ✕ │
│ [QR code]    │ ▪ report.pdf    C:\Users\me\Docs\report.pdf 1.2 MB  ⧉ ⌂ ✕ │
│ 2 connected  │                                                            │
│ 11.3 MB/s ▁▃▅│                 (drop files anywhere)                      │
├──────────────┴────────────────────────────────────────────────────────────┤
│ Port 2121 · Pool 2 items · Total 12.4 GB        12:01:07 DOWNLOAD a.mp4   │
└───────────────────────────────────────────────────────────────────────────┘
```

## How it works

1. Turn on your OS hotspot (Windows *Mobile hotspot*, macOS *Internet Sharing*, Linux NetworkManager *Hotspot*).
2. Launch the app. It scans local adapters and picks the hotspot gateway automatically
   (`192.168.137.1` on Windows, `192.168.2.1` on macOS, `10.42.0.1` on Linux). You can override it.
3. Add files/folders with the buttons or by dragging them onto the window.
4. Flip the **Server** switch. The FTP server binds to the hotspot IP on port `2121` (falls back to `2122`).
5. On the phone, scan the QR code or open `ftp://192.168.137.1:2121/Pool` in a browser or FTP client.

Everything you add appears under a single virtual `/Pool` directory, regardless of which drive it lives on. The share is **read-only**: uploads, deletes and renames are refused.

## Architecture

```
src-tauri/src
├── network.rs   interface scan + hotspot scoring (if-addrs)
├── pool.rs      in-memory pool, /Pool/<name> → host path mapping, dedupe
├── vfs.rs       read-only libunftp StorageBackend over the pool, byte counting
├── ftp.rs       server lifecycle, port fallback, Tauri events, MB/s sampler
└── lib.rs       Tauri commands: start_ftp, stop_ftp, get_ftp_status,
                 add_path_to_pool, remove_path_from_pool, get_pool,
                 get_network_interfaces, get_hotspot_interface
src
├── hooks/       useServer (status/stats/log listeners), usePool, useDragDrop
├── components/  TopBar, Sidebar (adapter, QR, metrics), PoolManager, StatusBar
└── lib/tauri.ts typed invoke/listen wrappers
```

Events emitted to the UI: `ftp_log`, `ftp_stats` (every 500 ms), `ftp_status`, `pool_changed`.

## Development

Prereqs: Rust stable, Node 22+, pnpm, and the [Tauri v2 system deps](https://v2.tauri.app/start/prerequisites/).

```bash
pnpm install
pnpm tauri dev                         # run with hot reload
cargo test --manifest-path src-tauri/Cargo.toml   # 8 tests incl. end-to-end curl against the real server
pnpm tauri build                       # production bundles in src-tauri/target/release/bundle
```

The release profile is tuned for size (`opt-level = "z"`, `lto`, `codegen-units = 1`, `panic = "abort"`, `strip`).
libunftp is compiled with TLS disabled apart from the mandatory `ring` provider.

## Firewall note

Windows may prompt to allow the app through the firewall the first time the server starts. Allow it on **private** networks. The server uses control port `2121` and passive data ports `50000–50100`.

## License

MIT
