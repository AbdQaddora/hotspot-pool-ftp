import { useEffect, useRef } from "react";
import { Database, HardDrive, Network, Terminal } from "lucide-react";
import { formatBytes, formatTime } from "@/lib/format";
import { cn } from "@/lib/utils";
import { DEFAULT_PORT, type ServerStatus, type LogEvent } from "@/lib/tauri";

interface Props {
  status: ServerStatus;
  totalSize: number;
  pending: boolean;
  itemCount: number;
  logs: LogEvent[];
}

const LEVEL_CLASS: Record<LogEvent["level"], string> = {
  info: "text-muted-foreground",
  success: "text-live",
  warn: "text-warn",
  error: "text-destructive",
};

export function StatusBar({ status, totalSize, pending, itemCount, logs }: Props) {
  const scroller = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = scroller.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [logs.length]);
  const recent = logs.slice(-40);

  return (
    <footer className="flex h-[88px] shrink-0 flex-col border-t border-border bg-background">
      <div className="flex h-7 shrink-0 items-center gap-4 border-b border-border px-3 text-[11px] text-muted-foreground">
        <span className="flex items-center gap-1.5">
          <Network className="size-3" />
          HTTP <span className={cn("font-mono", status.running ? "text-live" : "text-foreground")}>{status.port ?? DEFAULT_PORT}</span>
          {status.httpsPort && <span className="font-mono text-muted-foreground">· HTTPS {status.httpsPort}</span>}
        </span>
        <span className="flex items-center gap-1.5">
          <Database className="size-3" />
          Pool <span className="font-mono text-foreground">{itemCount}</span> items
        </span>
        <span className="flex items-center gap-1.5">
          <HardDrive className="size-3" />
          Total <span className="font-mono text-foreground">{formatBytes(totalSize)}</span>
          {pending && <span className="animate-pulse">(calculating…)</span>}
        </span>
        <span className="ml-auto flex items-center gap-1.5">
          <Terminal className="size-3" />
          Event log
        </span>
      </div>
      <div ref={scroller} className="min-h-0 flex-1 overflow-y-auto px-3 py-1 font-mono text-[10.5px] leading-[16px]">
        {recent.length === 0 ? (
          <div className="text-muted-foreground/60">Waiting for events…</div>
        ) : (
          recent.map((l, i) => (
            <div key={`${l.ts}-${i}`} className="flex gap-2 whitespace-nowrap">
              <span className="text-muted-foreground/60">{formatTime(l.ts)}</span>
              <span className={cn("w-[74px] shrink-0 uppercase", LEVEL_CLASS[l.level])}>{l.kind}</span>
              <span className="truncate text-foreground/90">{l.message}</span>
              {l.client && <span className="text-muted-foreground/60">#{l.client.slice(0, 6)}</span>}
            </div>
          ))
        )}
      </div>
    </footer>
  );
}
