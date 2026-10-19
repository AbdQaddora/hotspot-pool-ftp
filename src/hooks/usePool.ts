import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, events, type PoolItem } from "@/lib/tauri";

export function usePool() {
  const [items, setItems] = useState<PoolItem[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let unsub: (() => void) | undefined;
    api.getPool().then(setItems);
    events.onPool(setItems).then((u) => (unsub = u));
    return () => unsub?.();
  }, []);

  const addPaths = useCallback(async (paths: string[]) => {
    if (!paths.length) return;
    setError(null);
    try {
      setItems(await api.addPathToPool(paths));
    } catch (e) {
      setError(String(e));
      setItems(await api.getPool());
    }
  }, []);

  const addFiles = useCallback(async () => {
    const sel = await open({ multiple: true, directory: false, title: "Add files to pool" });
    if (sel) await addPaths(Array.isArray(sel) ? sel : [sel]);
  }, [addPaths]);

  const addFolders = useCallback(async () => {
    const sel = await open({ multiple: true, directory: true, title: "Add folders to pool" });
    if (sel) await addPaths(Array.isArray(sel) ? sel : [sel]);
  }, [addPaths]);

  const remove = useCallback(async (id: string) => {
    setItems(await api.removePathFromPool(id));
  }, []);

  const setPassword = useCallback(async (id: string, password: string | null) => {
    setError(null);
    try {
      setItems(await api.setPoolPassword(id, password));
      return true;
    } catch (e) {
      setError(String(e));
      return false;
    }
  }, []);

  const totalSize = items.reduce((a, i) => a + i.size, 0);
  const pending = items.some((i) => i.sizePending);

  return { items, totalSize, pending, error, addPaths, addFiles, addFolders, remove, setPassword, dismissError: () => setError(null) };
}
