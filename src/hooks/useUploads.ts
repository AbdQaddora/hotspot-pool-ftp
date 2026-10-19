import { useCallback, useEffect, useState } from "react";
import { api, events, type InboxInfo, type UploadItem } from "@/lib/tauri";

export function useUploads() {
  const [items, setItems] = useState<UploadItem[]>([]);
  const [info, setInfo] = useState<InboxInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const [list, inbox] = await Promise.all([api.listUploads(), api.getInboxInfo()]);
      setItems(list);
      setInfo(inbox);
    } catch {
      /* ignore */
    }
  }, []);

  useEffect(() => {
    let unsub: (() => void) | undefined;
    refresh();
    events.onUploads((list) => {
      setItems(list);
      api.getInboxInfo().then(setInfo).catch(() => {});
    }).then((u) => (unsub = u));
    const t = window.setInterval(refresh, 10000);
    return () => {
      unsub?.();
      window.clearInterval(t);
    };
  }, [refresh]);

  const run = useCallback(async (key: string, fn: () => Promise<UploadItem[]>) => {
    setBusy(key);
    setError(null);
    try {
      setItems(await fn());
      setInfo(await api.getInboxInfo());
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }, []);

  return {
    items,
    info,
    error,
    busy,
    refresh,
    approve: (id: string) => run(id, () => api.approveUpload(id)),
    reject: (id: string) => run(id, () => api.rejectUpload(id)),
    approveAll: () => run("all", api.approveAllUploads),
    rejectAll: () => run("all", api.rejectAllUploads),
    dismissError: () => setError(null),
  };
}
