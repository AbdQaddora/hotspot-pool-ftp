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
let hotspotOn = false;
const hotspotInfo = () => ({ supported: true, state: hotspotOn ? "on" : "off", ssid: "ABD-Laptop", passphrase: "pool-2026-share", clientCount: hotspotOn ? 2 : 0, wifiQr: "WIFI:T:WPA;S:ABD-Laptop;P:pool-2026-share;;", error: null });
let total = 0;
let peak = 0;
let connected = 0;
const pool = [
  { id: "p1", name: "Videos", path: "D:\\Media\\Videos", isDir: true, size: 13_300_000_000, sizePending: false, addedAt: Date.now(), protected: true },
  { id: "p2", name: "Lecture 04 - Signals.mp4", path: "D:\\Media\\Lectures\\Lecture 04 - Signals.mp4", isDir: false, size: 812_000_000, sizePending: false, addedAt: Date.now(), protected: false },
  { id: "p3", name: "Photos 2026", path: "C:\\Users\\abd\\Pictures\\Photos 2026", isDir: true, size: 0, sizePending: true, addedAt: Date.now(), protected: false },
  { id: "p4", name: "thesis-draft.pdf", path: "C:\\Users\\abd\\Documents\\thesis-draft.pdf", isDir: false, size: 4_200_000, sizePending: false, addedAt: Date.now(), protected: false },
  { id: "p5", name: "backup.zip", path: "E:\\Archive\\backup.zip", isDir: false, size: 2_950_000_000, sizePending: false, addedAt: Date.now(), protected: false },
];

const ifaces = [
  { name: "Local Area Connection* 12", ip: "192.168.137.1", isLoopback: false, isHotspot: true, score: 100, adapterId: "99999999-8888-7777-6666-555555555555" },
  { name: "Wi-Fi", ip: "192.168.1.42", isLoopback: false, isHotspot: false, score: 20, adapterId: "abcdef01-1234-5678-9abc-def012345678" },
  { name: "Loopback Pseudo-Interface 1", ip: "127.0.0.1", isLoopback: true, isHotspot: false, score: 0, adapterId: null },
];

let settings = { name: "abdu", suffix: "link", autoHotspot: true, allowUploads: true, uploadPin: "", maxUploadMb: 4096, inboxQuotaMb: 20480, inboxDir: null as string | null, upstreamAdapter: null as string | null, hotspotEngine: "wifidirect" as "wifidirect" | "mobile", shareInternet: true, normalizeTtl: false };
let inbox = [
  { id: "u1", name: "IMG_2041.jpg", originalName: "IMG_2041.jpg", size: 3_400_000, device: "Abd's iPhone", ip: "192.168.137.23", receivedAt: Date.now() - 120000, sha256: "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08", path: "C:\\Users\\abd\\AppData\\Roaming\\com.hotspotpool.ftp\\inbox\\u1_IMG_2041.jpg", executable: false },
  { id: "u2", name: "setup_tool.exe", originalName: "setup tool.exe", size: 48_000_000, device: "Galaxy S24", ip: "192.168.137.41", receivedAt: Date.now() - 30000, sha256: "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824", path: "C:\\Users\\abd\\AppData\\Roaming\\com.hotspotpool.ftp\\inbox\\u2_setup_tool.exe", executable: true },
  { id: "u3", name: "notes_lecture.pdf", originalName: "notes/lecture?.pdf", size: 1_200_000, device: "", ip: "192.168.137.52", receivedAt: Date.now() - 5000, sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", path: "C:\\Users\\abd\\AppData\\Roaming\\com.hotspotpool.ftp\\inbox\\u3_notes_lecture.pdf", executable: false },
];
const inboxInfo = () => ({ dir: "C:\\Users\\abd\\AppData\\Roaming\\com.hotspotpool.ftp\\inbox", usedBytes: inbox.reduce((a, i) => a + i.size, 0), quotaBytes: settings.inboxQuotaMb * 1024 * 1024, pending: inbox.length });
let nameResolves: boolean | null = null;
// Failure states for screenshots: ?nameFail (hosts write cancelled) and ?port=8080 (port 80 taken).
const mockFlags = new URLSearchParams(location.search);
const mockPort = Number(mockFlags.get("port")) || 80;
const mockNameFail = mockFlags.has("nameFail");
const mockUrl = (host: string) => `http://${host}${mockPort === 80 ? "" : `:${mockPort}`}`;
const nameError = () =>
  mockNameFail
    ? "The hosts file could not be updated (the admin prompt was cancelled), so phones cannot use the name yet."
    : !settings.shareInternet && settings.hotspotEngine === "wifidirect"
      ? "Share internet is off, so the hotspot has no DNS for phones and the name cannot work. Type the IP instead."
      : null;
let mockShared = false;
const upstreams = [
  { id: "abcdef01-1234-5678-9abc-def012345678", name: "CafeNet", kind: "wifi", connectivity: "internet", isDefault: true },
  { id: "11111111-2222-3333-4444-555555555555", name: "Ethernet", kind: "ethernet", connectivity: "none", isDefault: false },
];
const wifiStatus = () => {
  const effective = upstreams.find((u) => u.id === settings.upstreamAdapter)?.id ?? upstreams[0].id;
  const publicId = mockShared ? effective : null;
  const health = !hotspotOn ? "hotspot_off" : mockShared ? "sharing" : "not_shared";
  return {
    hotspot: hotspotInfo(),
    upstreams,
    selectedUpstream: settings.upstreamAdapter,
    effectiveUpstream: effective,
    sharing: {
      available: true,
      connections: [
        { id: upstreams[0].id, name: "Wi-Fi", connected: true, sharing: publicId === upstreams[0].id ? "public" : "none" },
        { id: upstreams[1].id, name: "Ethernet", connected: false, sharing: publicId === upstreams[1].id ? "public" : "none" },
        { id: ifaces[0].adapterId, name: ifaces[0].name, connected: hotspotOn, sharing: mockShared ? "private" : "none" },
      ],
      publicId,
      privateId: mockShared ? ifaces[0].adapterId : null,
      error: null,
      source: mockShared ? "last_applied" : "wmi",
    },
    hotspotAdapter: hotspotOn ? ifaces[0] : null,
    clients: hotspotOn ? [{ mac: "6C-88-14-16-41-94", name: null, ip: "192.168.137.23" }, { mac: "A4-50-46-7B-10-2C", name: "Galaxy-S24", ip: "192.168.137.41" }] : [],
    health: new URLSearchParams(location.search).has("apfail") && hotspotOn ? "ap_failed" : health,
    signInUrl: "http://www.msftconnecttest.com/redirect",
    engine: settings.hotspotEngine,
    apRunning: hotspotOn && settings.hotspotEngine === "wifidirect" && !new URLSearchParams(location.search).has("apfail"),
    apError: new URLSearchParams(location.search).has("apfail") ? "Another hotspot program is using the Wi-Fi adapter. Turn off Windows Mobile Hotspot and MyPublicWiFi, then try again." : null,
  };
};
const ftpStatus = () =>
  running
    ? { running: true, ip: "192.168.137.1", port: mockPort, url: mockUrl("192.168.137.1"), name: `${settings.name}.${settings.suffix}`, nameUrl: mockUrl(`${settings.name}.${settings.suffix}`), nameResolves: nameError() ? false : nameResolves, nameError: nameError(), hostnameUrl: nameResolves ? mockUrl("abd-laptop") : null }
    : { running: false, ip: null, port: null, url: null, name: `${settings.name}.${settings.suffix}`, nameUrl: null, nameResolves: null, nameError: null, hostnameUrl: null };
const log = (level: string, kind: string, message: string, extra: Record<string, unknown> = {}) =>
  emit("server_log", { ts: Date.now(), level, kind, message, client: null, path: null, bytes: null, ...extra });

let ticker: number | undefined;
function startSim() {
  running = true;
  connected = 2;
  total = 0;
  peak = 0;
  log("success", "server", `Sharing at ${mockUrl("192.168.137.1")}`);
  setTimeout(() => log("success", "connect", "Device 192.168.137.23 connected (1 online)", { client: "192.168.137.23" }), 300);
  setTimeout(() => log("success", "connect", "Device 192.168.137.41 connected (2 online)", { client: "192.168.137.41" }), 900);
  let t = 0;
  ticker = window.setInterval(() => {
    t += 0.5;
    const base = 9.5 + 3 * Math.sin(t / 2) + Math.random() * 1.5;
    const speed = base * 1024 * 1024;
    total += speed / 2;
    peak = Math.max(peak, speed);
    emit("server_stats", { speedBps: speed, peakBps: peak, connected, activeTransfers: 1, totalBytes: total });
    if (Math.round(t * 2) % 14 === 0) log("success", "download", "192.168.137.23 downloaded S01E03.mkv (1.4 GB)", { client: "192.168.137.23", path: "/Videos/S01E03.mkv", bytes: 1_400_000_000 });
  }, 500);
}
function stopSim() {
  running = false;
  connected = 0;
  if (ticker) window.clearInterval(ticker);
  emit("server_stats", { speedBps: 0, peakBps: 0, connected: 0, activeTransfers: 0, totalBytes: total });
  log("info", "server", "Server stopped");
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
    case "list_uploads":
      return inbox;
    case "get_inbox_info":
      return inboxInfo();
    case "approve_upload":
    case "reject_upload":
      inbox = inbox.filter((i) => i.id !== args.id);
      return inbox;
    case "approve_all_uploads":
    case "reject_all_uploads":
      inbox = [];
      return inbox;
    case "get_settings":
      return settings;
    case "set_settings":
      settings = { ...settings, ...(args.settings as object) };
      return settings;
    case "setup_name":
      await new Promise((r) => setTimeout(r, 400));
      nameResolves = !nameError();
      log(nameResolves ? "success" : "warn", "server", nameResolves ? `${settings.name}.${settings.suffix} resolves on the hotspot` : (nameError() as string));
      return ftpStatus();
    case "check_name":
      return ftpStatus();
    case "get_hotspot_info":
      return hotspotInfo();
    case "set_hotspot":
      hotspotOn = !!args.on;
      log("success", "server", hotspotOn ? (settings.hotspotEngine === "wifidirect" ? 'Access point "ABD-Laptop" is on' : 'Hotspot "ABD-Laptop" is on') : "Hotspot off");
      return hotspotInfo();
    case "configure_hotspot":
      return hotspotInfo();
    case "hotspot_settings_url":
      return "ms-settings:network-mobilehotspot";
    case "get_wifi_share_status":
      return wifiStatus();
    case "get_diagnostics":
      return "Hotspot Pool (mock)\nengine: wifidirect\nhealth: not_shared\n";
    case "get_ttl_state":
      return { available: true, running: !!settings.normalizeTtl && running, status: null };
    case "detect_ttl":
      return new URLSearchParams(location.search).has("ttlblock") ? 1 : 117;
    case "apply_internet_sharing":
      await new Promise((r) => setTimeout(r, 500));
      mockShared = true;
      log("success", "server", "Devices on the hotspot now use CafeNet for internet");
      return wifiStatus();
    case "auto_share": {
      const started = !hotspotOn;
      hotspotOn = true;
      await new Promise((r) => setTimeout(r, 300));
      startSim();
      nameResolves = !nameError();
      emit("server_status", ftpStatus());
      return { hotspot: hotspotInfo(), status: ftpStatus(), hotspotStarted: started };
    }
    case "stop_share":
      stopSim();
      if (args.turnOffHotspot) hotspotOn = false;
      emit("server_status", ftpStatus());
      return { hotspot: hotspotInfo(), status: ftpStatus(), hotspotStarted: false };
    case "get_server_status":
      return ftpStatus();
    case "start_server":
      startSim();
      emit("server_status", ftpStatus());
      return ftpStatus();
    case "stop_server":
      stopSim();
      emit("server_status", ftpStatus());
      return ftpStatus();
    case "get_pool":
      return pool;
    case "add_path_to_pool":
      return pool;
    case "set_pool_password": {
      const it = pool.find((p) => p.id === args.id);
      if (it) it.protected = !!args.password;
      return pool;
    }
    case "remove_path_from_pool": {
      const i = pool.findIndex((p) => p.id === args.id);
      if (i >= 0) pool.splice(i, 1);
      return pool;
    }
    case "plugin:window|is_maximized":
      return false;
    case "plugin:clipboard-manager|write_text":
    case "plugin:opener|reveal_item_in_dir":
    case "plugin:opener|open_url":
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
  setTimeout(() => invoke("auto_share", {}), 400);
}
export {};
