import { useEffect, useRef } from "react";
import { ArrowDownToLine, Gauge, Trash2, Users } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardBody, CardHeader, PageHeader } from "@/components/ui/card";
import { Sparkline } from "@/components/Sparkline";
import { formatBytes, formatSpeed, formatTime } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { LogEvent, ServerStatus, StatsEvent } from "@/lib/tauri";

const LEVEL_CLASS: Record<LogEvent["level"], string> = {
  info: "text-muted-foreground",
  success: "text-live",
  warn: "text-warn",
  error: "text-destructive",
};

function Tile({ label, value, sub, icon }: { label: string; value: string; sub?: string; icon: React.ReactNode }) {
  return (
    <Card>
      <CardBody className="px-4 py-3.5">
        <div className="flex items-center justify-between text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
          {label}
          <span className="[&_svg]:size-3.5">{icon}</span>
        </div>
        <div className="mt-1 font-mono text-[22px] font-semibold tabular-nums leading-none tracking-tight">{value}</div>
        {sub && <div className="mt-1 text-[11.5px] text-muted-foreground">{sub}</div>}
      </CardBody>
    </Card>
  );
}

export function ActivityTab({ status, stats, history, logs, onClear }: { status: ServerStatus; stats: StatsEvent; history: number[]; logs: LogEvent[]; onClear: () => void }) {
  const scroller = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = scroller.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [logs.length]);
  const downloads = logs.filter((l) => l.kind === "download" && l.level === "success").length;

  return (
    <div className="mx-auto flex h-full w-full max-w-[880px] flex-col">
      <PageHeader title="Activity" description={status.running ? "Live view of connected phones and transfers." : "Start sharing to see devices and transfers."} />
      <div className="mb-4 grid grid-cols-2 gap-3 lg:grid-cols-4">
        <Tile label="Devices" value={String(stats.connected)} sub="seen in the last 45 s" icon={<Users />} />
        <Tile label="Speed" value={formatSpeed(stats.speedBps)} sub={`peak ${formatSpeed(stats.peakBps)}`} icon={<Gauge />} />
        <Tile label="Served" value={formatBytes(stats.totalBytes)} sub="since the server started" icon={<ArrowDownToLine />} />
        <Tile label="Downloads" value={String(downloads)} sub={`${stats.activeTransfers} in progress`} icon={<ArrowDownToLine />} />
      </div>
      <Card className="mb-4">
        <CardHeader title="Throughput" description="Last 30 seconds, sampled twice a second." />
        <CardBody className="py-3">
          <Sparkline data={history} height={56} />
        </CardBody>
      </Card>
      <Card className="flex min-h-0 flex-1 flex-col">
        <CardHeader
          title="Event log"
          description="Connections, downloads and server events."
          right={
            <Button variant="ghost" size="sm" onClick={onClear} disabled={!logs.length}>
              <Trash2 /> Clear
            </Button>
          }
        />
        <div ref={scroller} className="min-h-[160px] flex-1 overflow-y-auto px-4 py-2 font-mono text-[11px] leading-[18px]">
          {logs.length === 0 ? (
            <div className="py-6 text-center text-muted-foreground/60">No events yet.</div>
          ) : (
            logs.map((l, i) => (
              <div key={`${l.ts}-${i}`} className="flex gap-3 whitespace-nowrap">
                <span className="text-muted-foreground/60">{formatTime(l.ts)}</span>
                <span className={cn("w-[76px] shrink-0 uppercase", LEVEL_CLASS[l.level])}>{l.kind}</span>
                <span className="truncate text-foreground/90">{l.message}</span>
              </div>
            ))
          )}
        </div>
      </Card>
    </div>
  );
}
