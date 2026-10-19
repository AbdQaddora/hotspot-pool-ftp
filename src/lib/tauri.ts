import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface NetInterface {
  name: string;
  ip: string;
  isLoopback: boolean;
  isHotspot: boolean;
  score: number;
}

export interface ServerStatus {
  running: boolean;
  ip: string | null;
  port: number | null;
  /** http://<ip>[:port] */
  url: string | null;
  /** Full share name, e.g. "abdu.link". */
  name: string | null;
  /** Link for the name, e.g. "http://abdu.link". */
  nameUrl: string | null;
  /** null = not checked yet. */
  nameResolves: boolean | null;
  /** http://<laptop-hostname>[:port] when the hotspot DNS resolves it. */
  hostnameUrl: string | null;
}

export interface Settings {
  name: string;
  /** A real top-level domain (link, site, net…) so phone browsers open it instead of searching. */
  suffix: string;
  autoHotspot: boolean;
}

/** Suffixes known to be real TLDs (from the public suffix list), offered as quick picks. */
export const SUFFIX_PICKS = ["link", "site", "net", "zone", "one"];

export interface PoolItem {
  id: string;
  name: string;
  path: string;
  isDir: boolean;
  size: number;
  sizePending: boolean;
  addedAt: number;
}

export type LogLevel = "info" | "success" | "warn" | "error";
export type LogKind = "server" | "connect" | "disconnect" | "download" | "error";

export interface LogEvent {
  ts: number;
  level: LogLevel;
  kind: LogKind;
  message: string;
  client: string | null;
  path: string | null;
  bytes: number | null;
}

export interface StatsEvent {
  speedBps: number;
  peakBps: number;
  connected: number;
  activeTransfers: number;
  totalBytes: number;
}

export interface HotspotInfo {
  supported: boolean;
  state: "on" | "off" | "transition" | "unknown" | "unsupported";
  ssid: string | null;
  passphrase: string | null;
  clientCount: number | null;
  wifiQr: string | null;
  error: string | null;
}

export interface ShareResult {
  hotspot: HotspotInfo;
  status: ServerStatus;
  hotspotStarted: boolean;
}

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (settings: Settings) => invoke<Settings>("set_settings", { settings }),
  setupName: () => invoke<ServerStatus>("setup_name"),
  checkName: () => invoke<ServerStatus>("check_name"),
  getHotspotInfo: () => invoke<HotspotInfo>("get_hotspot_info"),
  setHotspot: (on: boolean) => invoke<HotspotInfo>("set_hotspot", { on }),
  configureHotspot: (ssid: string, passphrase: string) => invoke<HotspotInfo>("configure_hotspot", { ssid, passphrase }),
  hotspotSettingsUrl: () => invoke<string>("hotspot_settings_url"),
  autoShare: (ip?: string) => invoke<ShareResult>("auto_share", { ip: ip ?? null }),
  stopShare: (turnOffHotspot: boolean) => invoke<ShareResult>("stop_share", { turnOffHotspot }),
  getNetworkInterfaces: () => invoke<NetInterface[]>("get_network_interfaces"),
  getHotspotInterface: () => invoke<NetInterface | null>("get_hotspot_interface"),
  startServer: (ip?: string, port?: number) => invoke<ServerStatus>("start_server", { ip: ip ?? null, port: port ?? null }),
  stopServer: () => invoke<ServerStatus>("stop_server"),
  getServerStatus: () => invoke<ServerStatus>("get_server_status"),
  getPool: () => invoke<PoolItem[]>("get_pool"),
  addPathToPool: (paths: string[]) => invoke<PoolItem[]>("add_path_to_pool", { paths }),
  removePathFromPool: (id: string) => invoke<PoolItem[]>("remove_path_from_pool", { id }),
};

export const events = {
  onLog: (cb: (e: LogEvent) => void): Promise<UnlistenFn> => listen<LogEvent>("server_log", (ev) => cb(ev.payload)),
  onStats: (cb: (e: StatsEvent) => void): Promise<UnlistenFn> => listen<StatsEvent>("server_stats", (ev) => cb(ev.payload)),
  onStatus: (cb: (e: ServerStatus) => void): Promise<UnlistenFn> => listen<ServerStatus>("server_status", (ev) => cb(ev.payload)),
  onPool: (cb: (e: PoolItem[]) => void): Promise<UnlistenFn> => listen<PoolItem[]>("pool_changed", (ev) => cb(ev.payload)),
};

export const DEFAULT_PORT = 80;

/** Base URL phones open, using the live server or the adapter that would be used. */
export function shareUrl(status: ServerStatus, fallbackIp: string | null): string | null {
  if (status.url) return status.url;
  return fallbackIp ? `http://${fallbackIp}` : null;
}

/** Wi-Fi join payload for QR codes (understood by iOS and Android cameras). */
export function wifiQr(ssid: string, passphrase: string | null): string {
  const esc = (s: string) => s.replace(/\\/g, "\\\\").replace(/;/g, "\\;").replace(/,/g, "\\,").replace(/:/g, "\\:").replace(/"/g, '\\"');
  return passphrase ? `WIFI:T:WPA;S:${esc(ssid)};P:${esc(passphrase)};;` : `WIFI:T:nopass;S:${esc(ssid)};;`;
}

/** Direct link for a pooled item: files download, folders open in the web page. */
export function shareLink(status: ServerStatus, item: PoolItem): string | null {
  if (!status.url) return null;
  const name = encodeURIComponent(item.name);
  return item.isDir ? `${status.url}/#/${name}` : `${status.url}/dl/${name}`;
}
