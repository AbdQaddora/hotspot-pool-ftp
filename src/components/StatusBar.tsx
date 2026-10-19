import { Database, HardDrive, Network } from "lucide-react";
import { formatBytes, formatTime } from "@/lib/format";
import { cn } from "@/lib/utils";
import { DEFAULT_PORT, type LogEvent, type ServerStatus } from "@/lib/tauri";

interface Props {
  status: ServerStatus;
  totalSize: number;
  pending: boolean;
  itemCount: number;
  last?: LogEvent;
}

export function StatusBar({ status, totalSize, pending, itemCount, last }: Props) {
  return (
    <footer className="flex h-7 shrink-0 items-center gap-4 border-t border-border bg-background px-3 text-[11px] text-muted-foreground">
      <span className="flex items-center gap-1.5">
        <Network className="size-3" />
        HTTP <span className={cn("font-mono", status.running ? "text-live" : "text-foreground")}>{status.port ?? DEFAULT_PORT}</span>
      </span>
      <span className="flex items-center gap-1.5">
        <Database className="size-3" />
        Pool <span className="font-mono text-foreground">{itemCount}</span> items
      </span>
      <span className="flex items-center gap-1.5">
        <HardDrive className="size-3" />
        <span className="font-mono text-foreground">{formatBytes(totalSize)}</span>
        {pending && <span className="animate-pulse">(calculating…)</span>}
      </span>
      {last && (
        <span className="ml-auto flex min-w-0 items-center gap-2 font-mono">
          <span className="text-muted-foreground/60">{formatTime(last.ts)}</span>
          <span className={cn("uppercase", last.level === "success" ? "text-live" : last.level === "warn" ? "text-warn" : last.level === "error" ? "text-destructive" : "")}>{last.kind}</span>
          <span className="truncate text-foreground/80">{last.message}</span>
        </span>
      )}
    </footer>
  );
}
