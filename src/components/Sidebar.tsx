import { QRCodeSVG } from "qrcode.react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Check, Copy, ExternalLink, Gauge, RefreshCw, Smartphone, Wifi, WifiOff } from "lucide-react";
import { useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Sparkline } from "@/components/Sparkline";
import { formatBytes, formatSpeed } from "@/lib/format";
import { cn } from "@/lib/utils";
import { DEFAULT_PORT, shareUrl, type ServerStatus, type NetInterface, type StatsEvent } from "@/lib/tauri";

interface Props {
  status: ServerStatus;
  stats: StatsEvent;
  history: number[];
  interfaces: NetInterface[];
  selectedIp: string | null;
  onSelectIp: (ip: string) => void;
  onRefresh: () => void;
}

function Section({ title, icon, children, right }: { title: string; icon: React.ReactNode; children: React.ReactNode; right?: React.ReactNode }) {
  return (
    <section className="rounded-lg border border-border bg-card">
      <header className="flex h-8 items-center gap-2 border-b border-border px-3">
        <span className="text-muted-foreground [&_svg]:size-3.5">{icon}</span>
        <span className="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">{title}</span>
        <span className="ml-auto">{right}</span>
      </header>
      <div className="p-2.5">{children}</div>
    </section>
  );
}

export function Sidebar({ status, stats, history, interfaces, selectedIp, onSelectIp, onRefresh }: Props) {
  const [copied, setCopied] = useState(false);
  const active = interfaces.find((i) => i.ip === (status.running ? status.ip : selectedIp));
  const displayIp = status.running ? status.ip : selectedIp;
  const displayPort = status.port ?? DEFAULT_PORT;
  const url = shareUrl(status, displayIp);

  const copy = async () => {
    if (!url) return;
    await writeText(url);
    setCopied(true);
    setTimeout(() => setCopied(false), 1200);
  };

  return (
    <aside className="flex w-[272px] shrink-0 flex-col gap-2.5 overflow-y-auto border-r border-border bg-background p-2.5">
      <Section
        title="Hotspot adapter"
        icon={active?.isHotspot ? <Wifi /> : <WifiOff />}
        right={
          <Button variant="ghost" size="xs" onClick={onRefresh} aria-label="Rescan interfaces">
            <RefreshCw />
          </Button>
        }
      >
        <div className="space-y-2.5">
          <div className="flex items-center justify-between">
            <span className="truncate text-[12px] text-muted-foreground" title={active?.name}>
              {active?.name ?? "No adapter"}
            </span>
            {active?.isHotspot ? <Badge variant="live">Hotspot</Badge> : active ? <Badge variant="warn">Not hotspot</Badge> : <Badge variant="outline">None</Badge>}
          </div>
          <div className={cn("flex items-center justify-between rounded-md border border-border bg-background px-2.5 py-2 font-mono text-[13px]", status.running && "border-live/40 glow-live")}>
            <span className={cn(status.running ? "text-live" : "text-foreground")}>
              {displayIp ?? "—"}
              <span className="text-muted-foreground">:{displayPort}</span>
            </span>
            <Button variant="ghost" size="xs" onClick={copy} disabled={!url} aria-label="Copy address">
              {copied ? <Check className="text-live" /> : <Copy />}
            </Button>
          </div>
          <select
            className="h-7 w-full rounded-md border border-border bg-background px-2 text-[12px] text-foreground outline-none focus:ring-1 focus:ring-ring disabled:opacity-50"
            value={selectedIp ?? ""}
            disabled={status.running}
            onChange={(e) => onSelectIp(e.target.value)}
          >
            {interfaces.length === 0 && <option value="">No interfaces found</option>}
            {interfaces.map((i) => (
              <option key={`${i.name}-${i.ip}`} value={i.ip}>
                {i.ip} · {i.name}
                {i.isHotspot ? " ★" : ""}
              </option>
            ))}
          </select>
          {!active?.isHotspot && !status.running && (
            <p className="text-[11px] leading-snug text-muted-foreground">
              No hotspot gateway detected. Turn on <span className="text-foreground">Mobile Hotspot</span> and rescan, or pick an adapter manually.
            </p>
          )}
        </div>
      </Section>

      <Section title="Scan to connect" icon={<Smartphone />}>
        <div className="flex flex-col items-center gap-2.5">
          <div className={cn("rounded-md border border-border bg-background p-2.5 transition-opacity", !status.running && "opacity-40")}>
            <QRCodeSVG value={url ?? "http://0.0.0.0:8080"} size={124} bgColor="transparent" fgColor="#fafafa" level="M" />
          </div>
          <code className="w-full truncate text-center font-mono text-[10.5px] text-muted-foreground" title={url ?? ""}>
            {url ?? "server offline"}
          </code>
          {status.running ? (
            <Button variant="outline" size="sm" className="w-full" onClick={() => url && openUrl(url)}>
              <ExternalLink /> Open in browser
            </Button>
          ) : (
            <p className="text-center text-[11px] text-muted-foreground">Start the server to activate the link.</p>
          )}
        </div>
      </Section>

      <Section
        title="Clients"
        icon={<Gauge />}
        right={
          <Badge variant={stats.connected > 0 ? "live" : "outline"}>
            <span className={cn("h-1.5 w-1.5 rounded-full", stats.connected > 0 ? "bg-live" : "bg-muted-foreground/40")} />
            {stats.connected} connected
          </Badge>
        }
      >
        <div className="space-y-2">
          <div className="flex items-baseline justify-between">
            <span className="font-mono text-[20px] font-semibold tabular-nums leading-none text-foreground">{formatSpeed(stats.speedBps)}</span>
            <span className="font-mono text-[10.5px] text-muted-foreground">peak {formatSpeed(stats.peakBps)}</span>
          </div>
          <Sparkline data={history} />
          <div className="grid grid-cols-2 gap-2 text-[11px]">
            <div className="rounded-md border border-border bg-background px-2 py-1.5">
              <div className="text-muted-foreground">Active transfers</div>
              <div className="font-mono text-[12.5px] tabular-nums text-foreground">{stats.activeTransfers}</div>
            </div>
            <div className="rounded-md border border-border bg-background px-2 py-1.5">
              <div className="text-muted-foreground">Served</div>
              <div className="font-mono text-[12.5px] tabular-nums text-foreground">{formatBytes(stats.totalBytes)}</div>
            </div>
          </div>
        </div>
      </Section>
    </aside>
  );
}
