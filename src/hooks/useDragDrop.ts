import { useEffect, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";

/** Native OS drag & drop onto the window (Tauri webview events). */
export function useDragDrop(onDrop: (paths: string[]) => void) {
  const [dragging, setDragging] = useState(false);

  useEffect(() => {
    let unsub: (() => void) | undefined;
    getCurrentWebview()
      .onDragDropEvent((ev) => {
        const p = ev.payload;
        if (p.type === "enter" || p.type === "over") setDragging(true);
        else if (p.type === "leave") setDragging(false);
        else if (p.type === "drop") {
          setDragging(false);
          onDrop(p.paths);
        }
      })
      .then((u) => (unsub = u));
    return () => unsub?.();
  }, [onDrop]);

  return dragging;
}
