import { useCallback, useEffect, useRef, useState } from "react";
import { api, type WifiShareStatus } from "@/lib/tauri";

const POLL_MS = 5000;

/** Live snapshot for the Share Wi-Fi tab; polls while `enabled` and re-reads whenever `revision` changes (e.g. the hotspot state). */
export function useWifiShare({ enabled, revision }: { enabled: boolean; revision?: string }) {
  const [status, setStatus] = useState<WifiShareStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inflight = useRef(false);

  const refresh = useCallback(async () => {
    if (inflight.current) return;
    inflight.current = true;
    try {
      setStatus(await api.getWifiShareStatus());
    } catch (e) {
      setError(String(e));
    } finally {
      inflight.current = false;
    }
  }, []);

  useEffect(() => {
    if (!enabled) return;
    refresh();
    const t = window.setInterval(refresh, POLL_MS);
    return () => window.clearInterval(t);
  }, [enabled, revision, refresh]);

  const applySharing = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      setStatus(await api.applyInternetSharing());
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  return { status, busy, error, refresh, applySharing, dismissError: () => setError(null) };
}
