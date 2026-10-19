import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Maximize2, Minimize2, Minus, Radio, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { cn } from "@/lib/utils";
import type { FtpStatus } from "@/lib/tauri";

interface Props {
  status: FtpStatus;
  busy: boolean;
  onToggle: () => void;
}

export function TopBar({ status, busy, onToggle }: Props) {
  const win = getCurrentWindow();
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    let unsub: (() => void) | undefined;
    win.isMaximized().then(setMaximized).catch(() => {});
    win.onResized(() => win.isMaximized().then(setMaximized).catch(() => {})).then((u) => (unsub = u));
    return () => unsub?.();
  }, [win]);

  return (
    <header data-tauri-drag-region className="relative flex h-11 shrink-0 items-center border-b border-border bg-background/95 px-3 select-none">
      <div data-tauri-drag-region className="flex items-center gap-2.5">
        <div className={cn("flex h-6 w-6 items-center justify-center rounded-md border border-border bg-secondary", status.running && "border-live/40 glow-live")}>
          <Radio className={cn("size-3.5", status.running ? "text-live" : "text-muted-foreground")} />
        </div>
        <div data-tauri-drag-region className="flex items-baseline gap-2">
          <span data-tauri-drag-region className="text-[13px] font-semibold tracking-tight">
            Hotspot Pool FTP
          </span>
          <span data-tauri-drag-region className="font-mono text-[10.5px] text-muted-foreground">
            v0.1.0
          </span>
        </div>
      </div>

      <div data-tauri-drag-region className="flex-1" />

      <div className="flex items-center gap-3 rounded-md border border-border bg-card px-2.5 py-1">
        <div className="flex items-center gap-2">
          <span className="relative flex h-2 w-2">
            {status.running && <span className="ping absolute inset-0 rounded-full" />}
            <span className={cn("relative inline-flex h-2 w-2 rounded-full", status.running ? "bg-live" : "bg-muted-foreground/40")} />
          </span>
          <span className={cn("text-[11.5px] font-medium", status.running ? "text-live" : "text-muted-foreground")}>
            {busy ? "…" : status.running ? "SERVER ON" : "SERVER OFF"}
          </span>
        </div>
        <Switch checked={status.running} disabled={busy} onCheckedChange={onToggle} aria-label="Toggle FTP server" />
      </div>

      <div className="ml-3 flex items-center gap-0.5">
        <Button variant="ghost" size="xs" onClick={() => win.minimize()} aria-label="Minimize">
          <Minus />
        </Button>
        <Button variant="ghost" size="xs" onClick={() => win.toggleMaximize()} aria-label="Maximize">
          {maximized ? <Minimize2 /> : <Maximize2 />}
        </Button>
        <Button variant="ghost" size="xs" className="hover:bg-destructive/20 hover:text-destructive" onClick={() => win.close()} aria-label="Close">
          <X />
        </Button>
      </div>
    </header>
  );
}
