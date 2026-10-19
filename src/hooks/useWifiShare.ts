import { useCallback, useEffect, useRef, useState } from "react";
import { api, type WifiShareStatus } from "@/lib/tauri";

const POLL_MS = 5000;

/**
 * Live snapshot for the Share Wi-Fi tab. Polls while `enabled` (cheap, cached
 * sharing state) and re-reads fresh whenever `revision` changes, e.g. the
 * hotspot state or the chosen upstream. A change that lands while a read is in
 * flight queues one more read instead of being dropped.
 */
export function useWifiShare({ enabled, revision }: { enabled: boolean; revision?: string }) {
  const [status, setStatus] = useState<WifiShareStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inflight = useRef(false);
  const queued = useRef<boolean | null>(null);

  const refresh = useCallback(async (fresh = false) => {
    if (inflight.current) {
      queued.current = queued.current || fresh;
      return;
    }
    inflight.current = true;
    try {
      setStatus(await api.getWifiShareStatus(fresh));
    } catch (e) {
      setError(String(e));
    } finally {
      inflight.current = false;
      if (queued.current !== null) {
        const again = queued.current;
        queued.current = null;
        void refresh(again);
      }
    }
  }, []);

  useEffect(() => {
    if (!enabled) return;
    void refresh(true);
    const t = window.setInterval(() => void refresh(false), POLL_MS);
    return () => window.clearInterval(t);
  }, [enabled, revision, refresh]);

  const applySharing = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      setStatus(await api.applyInternetSharing());
    } catch (e) {
      setError(String(e));
      void refresh(true);
    } finally {
      setBusy(false);
    }
  }, [refresh]);

  return { status, busy, error, refresh: () => void refresh(true), applySharing, dismissError: () => setError(null) };
}
