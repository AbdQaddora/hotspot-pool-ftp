import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface NetInterface {
  name: string;
  ip: string;
  isLoopback: boolean;
  isHotspot: boolean;
  score: number;
}

export interface FtpStatus {
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
  startFtp: (ip?: string, port?: number) => invoke<FtpStatus>("start_ftp", { ip: ip ?? null, port: port ?? null }),
  stopFtp: () => invoke<FtpStatus>("stop_ftp"),
  getFtpStatus: () => invoke<FtpStatus>("get_ftp_status"),
  getPool: () => invoke<PoolItem[]>("get_pool"),
  addPathToPool: (paths: string[]) => invoke<PoolItem[]>("add_path_to_pool", { paths }),
  removePathFromPool: (id: string) => invoke<PoolItem[]>("remove_path_from_pool", { id }),
};

export const events = {
  onLog: (cb: (e: LogEvent) => void): Promise<UnlistenFn> => listen<LogEvent>("ftp_log", (ev) => cb(ev.payload)),
  onStats: (cb: (e: StatsEvent) => void): Promise<UnlistenFn> => listen<StatsEvent>("ftp_stats", (ev) => cb(ev.payload)),
  onStatus: (cb: (e: FtpStatus) => void): Promise<UnlistenFn> => listen<FtpStatus>("ftp_status", (ev) => cb(ev.payload)),
  onPool: (cb: (e: PoolItem[]) => void): Promise<UnlistenFn> => listen<PoolItem[]>("pool_changed", (ev) => cb(ev.payload)),
};

/** Virtual FTP link for a pooled item. */
export function ftpLink(status: FtpStatus, item: PoolItem): string | null {
  if (!status.ip || !status.port) return null;
  const name = item.name.split("/").map(encodeURIComponent).join("/");
  return `ftp://${status.ip}:${status.port}/Pool/${name}${item.isDir ? "/" : ""}`;
}
