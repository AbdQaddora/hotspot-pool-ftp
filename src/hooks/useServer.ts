import { useCallback, useEffect, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, events, type HotspotInfo, type ServerStatus, type Settings, type LogEvent, type NetInterface, type StatsEvent } from "@/lib/tauri";

const MAX_LOG = 400;
const SPARK_POINTS = 60;

const idleStats: StatsEvent = { speedBps: 0, peakBps: 0, connected: 0, activeTransfers: 0, totalBytes: 0 };

export function useServer() {
  const [status, setStatus] = useState<ServerStatus>({ running: false, ip: null, port: null, url: null, httpsPort: null, name: null, nameUrl: null, nameResolves: null, hostnameUrl: null });
  const [settings, setSettingsState] = useState<Settings>({ name: "share", preferHttps: true, autoHotspot: true });
  const [nameBusy, setNameBusy] = useState(false);
  const [stats, setStats] = useState<StatsEvent>(idleStats);
  const [history, setHistory] = useState<number[]>(() => Array(SPARK_POINTS).fill(0));
  const [logs, setLogs] = useState<LogEvent[]>([]);
  const [interfaces, setInterfaces] = useState<NetInterface[]>([]);
  const [selectedIp, setSelectedIp] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [phase, setPhase] = useState<"idle" | "hotspot" | "server" | "stopping">("idle");
  const [error, setError] = useState<string | null>(null);
  const [hotspot, setHotspot] = useState<HotspotInfo>({ supported: false, state: "unknown", ssid: null, passphrase: null, clientCount: null, wifiQr: null, error: null });
  const [hotspotStartedByUs, setHotspotStartedByUs] = useState(false);
  const autoRef = useRef(true);
  autoRef.current = settings.autoHotspot;
  const startedRef = useRef(false);
  startedRef.current = hotspotStartedByUs;
  const selectedRef = useRef<string | null>(null);
  selectedRef.current = selectedIp;

  const refreshHotspot = useCallback(async () => {
    try {
      setHotspot(await api.getHotspotInfo());
    } catch {
      /* ignore */
    }
  }, []);

  const refreshInterfaces = useCallback(async () => {
    const list = await api.getNetworkInterfaces();
    setInterfaces(list);
    const best = list.find((i) => !i.isLoopback);
    setSelectedIp((prev) => (prev && list.some((i) => i.ip === prev) ? prev : (best?.ip ?? null)));
  }, []);

  useEffect(() => {
    let alive = true;
    const unsubs: Array<() => void> = [];
    (async () => {
      await Promise.all([refreshInterfaces(), refreshHotspot()]);
      try {
        const st = await api.getSettings();
        if (alive) setSettingsState(st);
      } catch {
        /* ignore */
      }
      const s = await api.getServerStatus();
      if (alive) setStatus(s);
      unsubs.push(
        await events.onStatus((e) => {
          setStatus(e);
          refreshHotspot();
        }),
      );
      unsubs.push(
        await events.onStats((e) => {
          setStats(e);
          setHistory((h) => [...h.slice(1), e.speedBps]);
        }),
      );
      unsubs.push(
        await events.onLog((e) => {
          setLogs((l) => {
            const next = [...l, e];
            return next.length > MAX_LOG ? next.slice(next.length - MAX_LOG) : next;
          });
        }),
      );
    })();
    const timer = window.setInterval(() => {
      refreshInterfaces();
      refreshHotspot();
    }, 8000);
    return () => {
      alive = false;
      window.clearInterval(timer);
      unsubs.forEach((u) => u());
    };
  }, [refreshInterfaces, refreshHotspot]);

  const fail = (e: unknown) => {
    setError(String(e));
    setLogs((l) => [...l, { ts: Date.now(), level: "error", kind: "error", message: String(e), client: null, path: null, bytes: null }]);
  };

  /** One click: hotspot on (when automated), wait for its adapter, start the server. */
  const start = useCallback(async () => {
    setBusy(true);
    setError(null);
    setPhase(autoRef.current ? "hotspot" : "server");
    try {
      // When the user picked an adapter that already is a hotspot gateway, bind to it directly.
      const pickIp = selectedRef.current ?? undefined;
      const r = autoRef.current ? await api.autoShare(undefined) : await api.autoShare(pickIp);
      setStatus(r.status);
      setHotspot(r.hotspot);
      setHotspotStartedByUs(r.hotspotStarted);
      setHistory(Array(SPARK_POINTS).fill(0));
      await refreshInterfaces();
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
      setPhase("idle");
    }
  }, [refreshInterfaces]);

  const stop = useCallback(async () => {
    setBusy(true);
    setPhase("stopping");
    try {
      const r = await api.stopShare(autoRef.current && startedRef.current);
      setStatus(r.status);
      setHotspot(r.hotspot);
      setHotspotStartedByUs(false);
      setStats(idleStats);
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
      setPhase("idle");
    }
  }, []);

  const toggleHotspot = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      setHotspot(await api.setHotspot(hotspot.state !== "on"));
      await refreshInterfaces();
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  }, [hotspot.state, refreshInterfaces]);

  const configureHotspot = useCallback(async (ssid: string, passphrase: string) => {
    setError(null);
    try {
      setHotspot(await api.configureHotspot(ssid, passphrase));
      return true;
    } catch (e) {
      fail(e);
      return false;
    }
  }, []);

  const saveSettings = useCallback(async (next: Settings) => {
    setError(null);
    try {
      const saved = await api.setSettings(next);
      setSettingsState(saved);
      setStatus(await api.getServerStatus());
      return true;
    } catch (e) {
      fail(e);
      return false;
    }
  }, []);

  const setupName = useCallback(async () => {
    setNameBusy(true);
    setError(null);
    try {
      setStatus(await api.setupName());
    } catch (e) {
      fail(e);
    } finally {
      setNameBusy(false);
    }
  }, []);

  const checkName = useCallback(async () => {
    setNameBusy(true);
    try {
      setStatus(await api.checkName());
    } catch (e) {
      fail(e);
    } finally {
      setNameBusy(false);
    }
  }, []);

  const openHotspotSettings = useCallback(async () => {
    const url = await api.hotspotSettingsUrl();
    if (url) await openUrl(url);
  }, []);

  const toggle = useCallback(() => (status.running ? stop() : start()), [status.running, start, stop]);

  return {
    status, stats, history, logs, interfaces, selectedIp, setSelectedIp, busy, phase, error, start, stop, toggle, refreshInterfaces,
    hotspot, toggleHotspot, configureHotspot, openHotspotSettings, refreshHotspot,
    settings, saveSettings, setupName, checkName, nameBusy,
    clearLogs: () => setLogs([]), dismissError: () => setError(null),
  };
}
