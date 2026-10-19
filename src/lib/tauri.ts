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
  url: string | null;
}

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

export const api = {
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

export const DEFAULT_PORT = 8080;

/** Base URL phones open, using the live server or the adapter that would be used. */
export function shareUrl(status: ServerStatus, fallbackIp: string | null): string | null {
  if (status.url) return status.url;
  return fallbackIp ? `http://${fallbackIp}:${DEFAULT_PORT}` : null;
}

/** Direct link for a pooled item: files download, folders open in the web page. */
export function shareLink(status: ServerStatus, item: PoolItem): string | null {
  if (!status.url) return null;
  const name = encodeURIComponent(item.name);
  return item.isDir ? `${status.url}/#/${name}` : `${status.url}/dl/${name}`;
}
