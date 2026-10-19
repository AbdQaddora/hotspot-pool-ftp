/**
 * Browser-only mock of the Tauri IPC bridge so the UI can be developed and
 * screenshotted outside the Tauri shell. Enabled with VITE_TAURI_MOCK=1.
 * Not included in production builds.
 */
type Handler = (payload: unknown) => void;

const listeners = new Map<string, Set<Handler>>();
const callbacks = new Map<number, (...args: unknown[]) => void>();
let nextCb = 1;

function emit(event: string, payload: unknown) {
  listeners.get(event)?.forEach((h) => h(payload));
}

let running = false;
let total = 0;
let peak = 0;
let connected = 0;
const pool = [
  { id: "p1", name: "Videos", path: "D:\\Media\\Videos", isDir: true, size: 13_300_000_000, sizePending: false, addedAt: Date.now() },
  { id: "p2", name: "Lecture 04 - Signals.mp4", path: "D:\\Media\\Lectures\\Lecture 04 - Signals.mp4", isDir: false, size: 812_000_000, sizePending: false, addedAt: Date.now() },
  { id: "p3", name: "Photos 2026", path: "C:\\Users\\abd\\Pictures\\Photos 2026", isDir: true, size: 0, sizePending: true, addedAt: Date.now() },
  { id: "p4", name: "thesis-draft.pdf", path: "C:\\Users\\abd\\Documents\\thesis-draft.pdf", isDir: false, size: 4_200_000, sizePending: false, addedAt: Date.now() },
  { id: "p5", name: "backup.zip", path: "E:\\Archive\\backup.zip", isDir: false, size: 2_950_000_000, sizePending: false, addedAt: Date.now() },
];

const ifaces = [
  { name: "Local Area Connection* 12", ip: "192.168.137.1", isLoopback: false, isHotspot: true, score: 100 },
  { name: "Wi-Fi", ip: "192.168.1.42", isLoopback: false, isHotspot: false, score: 20 },
  { name: "Loopback Pseudo-Interface 1", ip: "127.0.0.1", isLoopback: true, isHotspot: false, score: 0 },
];

const ftpStatus = () => (running ? { running: true, ip: "192.168.137.1", port: 2121, url: "ftp://192.168.137.1:2121/Pool" } : { running: false, ip: null, port: null, url: null });
const log = (level: string, kind: string, message: string, extra: Record<string, unknown> = {}) =>
  emit("ftp_log", { ts: Date.now(), level, kind, message, client: null, path: null, bytes: null, ...extra });

let ticker: number | undefined;
function startSim() {
  running = true;
  connected = 2;
  total = 0;
  peak = 0;
  log("success", "server", "FTP server listening on ftp://192.168.137.1:2121/Pool");
  setTimeout(() => log("success", "connect", "Client connected (1 online)", { client: "a81f3c9e" }), 300);
  setTimeout(() => log("success", "connect", "Client connected (2 online)", { client: "7bc02d11" }), 900);
  let t = 0;
  ticker = window.setInterval(() => {
    t += 0.5;
    const base = 9.5 + 3 * Math.sin(t / 2) + Math.random() * 1.5;
    const speed = base * 1024 * 1024;
    total += speed / 2;
    peak = Math.max(peak, speed);
    emit("ftp_stats", { speedBps: speed, peakBps: peak, connected, activeTransfers: 1, totalBytes: total });
    if (Math.round(t * 2) % 14 === 0) log("success", "download", "Downloaded /Pool/Videos/S01E03.mkv (1.4 GB)", { client: "a81f3c9e", path: "/Pool/Videos/S01E03.mkv", bytes: 1_400_000_000 });
  }, 500);
}
function stopSim() {
  running = false;
  connected = 0;
  if (ticker) window.clearInterval(ticker);
  emit("ftp_stats", { speedBps: 0, peakBps: 0, connected: 0, activeTransfers: 0, totalBytes: total });
  log("info", "server", "FTP server stopped");
}

async function invoke(cmd: string, args: Record<string, unknown> = {}): Promise<unknown> {
  switch (cmd) {
    case "plugin:event|listen": {
      const cb = callbacks.get(args.handler as number)!;
      const set = listeners.get(args.event as string) ?? new Set();
      set.add((payload) => cb({ event: args.event, id: args.handler, payload }));
      listeners.set(args.event as string, set);
      return args.handler;
    }
    case "plugin:event|unlisten":
      return;
    case "get_network_interfaces":
      return ifaces;
    case "get_hotspot_interface":
      return ifaces[0];
    case "get_ftp_status":
      return ftpStatus();
    case "start_ftp":
      startSim();
      emit("ftp_status", ftpStatus());
      return ftpStatus();
    case "stop_ftp":
      stopSim();
      emit("ftp_status", ftpStatus());
      return ftpStatus();
    case "get_pool":
      return pool;
    case "add_path_to_pool":
      return pool;
    case "remove_path_from_pool": {
      const i = pool.findIndex((p) => p.id === args.id);
      if (i >= 0) pool.splice(i, 1);
      return pool;
    }
    case "plugin:window|is_maximized":
      return false;
    case "plugin:clipboard-manager|write_text":
    case "plugin:opener|reveal_item_in_dir":
    case "plugin:window|minimize":
    case "plugin:window|toggle_maximize":
    case "plugin:window|close":
      return;
    case "plugin:dialog|open":
      return null;
    default:
      console.warn("[tauri-mock] unhandled command", cmd, args);
      return null;
  }
}

(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
  invoke,
  transformCallback(cb: (...a: unknown[]) => void) {
    const id = nextCb++;
    callbacks.set(id, cb);
    return id;
  },
  unregisterCallback(id: number) {
    callbacks.delete(id);
  },
  convertFileSrc: (p: string) => p,
  metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main", windowLabel: "main" }, windows: [{ label: "main" }], webviews: [{ label: "main", windowLabel: "main" }] },
};

// Auto-start the server after load when ?autostart is present (used for screenshots).
if (new URLSearchParams(location.search).has("autostart")) {
  setTimeout(() => invoke("start_ftp"), 400);
}
export {};
