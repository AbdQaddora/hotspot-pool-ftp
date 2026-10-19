import { useCallback, useEffect, useRef, useState } from "react";
import { api, events, type FtpStatus, type LogEvent, type NetInterface, type StatsEvent } from "@/lib/tauri";

const MAX_LOG = 400;
const SPARK_POINTS = 60;

const idleStats: StatsEvent = { speedBps: 0, peakBps: 0, connected: 0, activeTransfers: 0, totalBytes: 0 };

export function useServer() {
  const [status, setStatus] = useState<FtpStatus>({ running: false, ip: null, port: null, url: null });
  const [stats, setStats] = useState<StatsEvent>(idleStats);
  const [history, setHistory] = useState<number[]>(() => Array(SPARK_POINTS).fill(0));
  const [logs, setLogs] = useState<LogEvent[]>([]);
  const [interfaces, setInterfaces] = useState<NetInterface[]>([]);
  const [selectedIp, setSelectedIp] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const selectedRef = useRef<string | null>(null);
  selectedRef.current = selectedIp;

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
      await refreshInterfaces();
      const s = await api.getFtpStatus();
      if (alive) setStatus(s);
      unsubs.push(await events.onStatus((e) => setStatus(e)));
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
    const timer = window.setInterval(refreshInterfaces, 8000);
    return () => {
      alive = false;
      window.clearInterval(timer);
      unsubs.forEach((u) => u());
    };
  }, [refreshInterfaces]);

  const start = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      const s = await api.startFtp(selectedRef.current ?? undefined);
      setStatus(s);
      setHistory(Array(SPARK_POINTS).fill(0));
    } catch (e) {
      setError(String(e));
      setLogs((l) => [...l, { ts: Date.now(), level: "error", kind: "error", message: String(e), client: null, path: null, bytes: null }]);
    } finally {
      setBusy(false);
    }
  }, []);

  const stop = useCallback(async () => {
    setBusy(true);
    try {
      const s = await api.stopFtp();
      setStatus(s);
      setStats(idleStats);
    } finally {
      setBusy(false);
    }
  }, []);

  const toggle = useCallback(() => (status.running ? stop() : start()), [status.running, start, stop]);

  return { status, stats, history, logs, interfaces, selectedIp, setSelectedIp, busy, error, start, stop, toggle, refreshInterfaces, clearLogs: () => setLogs([]) };
}
