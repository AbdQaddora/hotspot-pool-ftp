import { QRCodeSVG } from "qrcode.react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Check, Copy, ExternalLink, Eye, EyeOff, Gauge, Pencil, Power, RefreshCw, Settings2, Smartphone, Wifi, WifiOff } from "lucide-react";
import { useEffect, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Sparkline } from "@/components/Sparkline";
import { formatBytes, formatSpeed } from "@/lib/format";
import { cn } from "@/lib/utils";
import { DEFAULT_PORT, shareUrl, wifiQr, type HotspotInfo, type ServerStatus, type NetInterface, type StatsEvent } from "@/lib/tauri";

interface Props {
  status: ServerStatus;
  stats: StatsEvent;
  history: number[];
  interfaces: NetInterface[];
  selectedIp: string | null;
  onSelectIp: (ip: string) => void;
  onRefresh: () => void;
  hotspot: HotspotInfo;
  busy: boolean;
  autoHotspot: boolean;
  onAutoHotspot: (v: boolean) => void;
  onToggleHotspot: () => void;
  onConfigureHotspot: (ssid: string, passphrase: string) => Promise<boolean>;
  onOpenHotspotSettings: () => void;
}

function Section({ title, icon, children, right }: { title: string; icon: React.ReactNode; children: React.ReactNode; right?: React.ReactNode }) {
  return (
    <section className="rounded-lg border border-border bg-card">
      <header className="flex h-8 items-center gap-2 border-b border-border px-3">
        <span className="text-muted-foreground [&_svg]:size-3.5">{icon}</span>
        <span className="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">{title}</span>
        <span className="ml-auto flex items-center gap-1">{right}</span>
      </header>
      <div className="p-2.5">{children}</div>
    </section>
  );
}

function CopyBtn({ value, label }: { value: string | null; label: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <Button
      variant="ghost"
      size="xs"
      disabled={!value}
      aria-label={label}
      onClick={async () => {
        if (!value) return;
        await writeText(value);
        setCopied(true);
        setTimeout(() => setCopied(false), 1200);
      }}
    >
      {copied ? <Check className="text-live" /> : <Copy />}
    </Button>
  );
}

function HotspotCredentials({ hotspot, onConfigure, disabled }: { hotspot: HotspotInfo; onConfigure: Props["onConfigureHotspot"]; disabled: boolean }) {
  const [show, setShow] = useState(false);
  const [editing, setEditing] = useState(false);
  const [ssid, setSsid] = useState(hotspot.ssid ?? "");
  const [pass, setPass] = useState(hotspot.passphrase ?? "");
  useEffect(() => {
    if (!editing) {
      setSsid(hotspot.ssid ?? "");
      setPass(hotspot.passphrase ?? "");
    }
  }, [hotspot.ssid, hotspot.passphrase, editing]);

  if (editing) {
    return (
      <form
        className="space-y-1.5"
        onSubmit={async (e) => {
          e.preventDefault();
          if (await onConfigure(ssid, pass)) setEditing(false);
        }}
      >
        <input className="h-7 w-full rounded-md border border-border bg-background px-2 text-[12px] outline-none focus:ring-1 focus:ring-ring" value={ssid} onChange={(e) => setSsid(e.target.value)} placeholder="Hotspot name" maxLength={32} />
        <input className="h-7 w-full rounded-md border border-border bg-background px-2 font-mono text-[12px] outline-none focus:ring-1 focus:ring-ring" value={pass} onChange={(e) => setPass(e.target.value)} placeholder="Password (8+ chars)" minLength={8} maxLength={63} />
        <div className="flex gap-1.5">
          <Button type="submit" size="sm" className="flex-1">Save</Button>
          <Button type="button" size="sm" variant="outline" onClick={() => setEditing(false)}>Cancel</Button>
        </div>
      </form>
    );
  }
  return (
    <div className="rounded-md border border-border bg-background px-2.5 py-1.5 text-[12px]">
      <div className="flex items-center justify-between gap-2">
        <span className="truncate text-muted-foreground">Network</span>
        <span className="truncate font-medium" title={hotspot.ssid ?? ""}>{hotspot.ssid ?? "—"}</span>
      </div>
      <div className="mt-1 flex items-center justify-between gap-2">
        <span className="text-muted-foreground">Password</span>
        <span className="flex items-center gap-0.5">
          <span className="font-mono">{hotspot.passphrase ? (show ? hotspot.passphrase : "•".repeat(Math.min(hotspot.passphrase.length, 12))) : "none"}</span>
          <Button variant="ghost" size="xs" onClick={() => setShow((v) => !v)} aria-label={show ? "Hide password" : "Show password"} disabled={!hotspot.passphrase}>
            {show ? <EyeOff /> : <Eye />}
          </Button>
          <CopyBtn value={hotspot.passphrase} label="Copy password" />
          <Button variant="ghost" size="xs" onClick={() => setEditing(true)} aria-label="Edit hotspot name and password" disabled={disabled}>
            <Pencil />
          </Button>
        </span>
      </div>
    </div>
  );
}

export function Sidebar({ status, stats, history, interfaces, selectedIp, onSelectIp, onRefresh, hotspot, busy, autoHotspot, onAutoHotspot, onToggleHotspot, onConfigureHotspot, onOpenHotspotSettings }: Props) {
  const [qrTab, setQrTab] = useState<"wifi" | "page">("wifi");
  const active = interfaces.find((i) => i.ip === (status.running ? status.ip : selectedIp));
  const displayIp = status.running ? status.ip : selectedIp;
  const displayPort = status.port ?? DEFAULT_PORT;
  const url = shareUrl(status, displayIp);
  const hotspotOn = hotspot.state === "on";
  const canJoinQr = !!hotspot.ssid;
  const tab = canJoinQr ? qrTab : "page";

  return (
    <aside className="flex w-[272px] shrink-0 flex-col gap-2.5 overflow-y-auto border-r border-border bg-background p-2.5">
      <Section
        title="Hotspot"
        icon={hotspotOn || active?.isHotspot ? <Wifi /> : <WifiOff />}
        right={
          <>
            {hotspot.supported ? (
              <Badge className="whitespace-nowrap" variant={hotspotOn ? "live" : hotspot.state === "transition" ? "warn" : "outline"}>
                <span className={cn("h-1.5 w-1.5 rounded-full", hotspotOn ? "bg-live" : "bg-muted-foreground/40")} />
                {hotspotOn ? "On" : hotspot.state === "transition" ? "Switching…" : "Off"}
                {hotspotOn && hotspot.clientCount != null && ` · ${hotspot.clientCount}`}
              </Badge>
            ) : (
              <Badge className="whitespace-nowrap" variant="outline">Manual</Badge>
            )}
            <Button variant="ghost" size="xs" onClick={onRefresh} aria-label="Rescan" disabled={busy}>
              <RefreshCw />
            </Button>
          </>
        }
      >
        <div className="space-y-2">
          {hotspot.supported ? (
            <>
              <HotspotCredentials hotspot={hotspot} onConfigure={onConfigureHotspot} disabled={busy || hotspotOn} />
              <div className="flex gap-1.5">
                <Button variant={hotspotOn ? "outline" : "live"} size="sm" className="flex-1" onClick={onToggleHotspot} disabled={busy || status.running}>
                  <Power /> {hotspotOn ? "Turn off hotspot" : "Turn on hotspot"}
                </Button>
                <Button variant="ghost" size="sm" onClick={onOpenHotspotSettings} aria-label="Open Windows hotspot settings">
                  <Settings2 />
                </Button>
              </div>
              <label className="flex cursor-pointer items-center gap-2 text-[11.5px] text-muted-foreground">
                <input type="checkbox" className="accent-[var(--live)]" checked={autoHotspot} onChange={(e) => onAutoHotspot(e.target.checked)} disabled={busy} />
                Manage the hotspot with the Server switch
              </label>
            </>
          ) : (
            <>
              <p className="text-[11.5px] leading-snug text-muted-foreground">{hotspot.error ?? "Turn on your hotspot in the system settings, then rescan."}</p>
              <Button variant="outline" size="sm" className="w-full" onClick={onOpenHotspotSettings}>
                <Settings2 /> Open hotspot settings
              </Button>
            </>
          )}

          <div className="border-t border-border pt-2">
            <div className="mb-1 flex items-center justify-between">
              <span className="min-w-0 truncate text-[11.5px] text-muted-foreground" title={active?.name}>
                {active?.name ?? "No adapter"}
              </span>
              {active?.isHotspot ? <Badge variant="live" className="shrink-0">Hotspot</Badge> : active ? <Badge variant="warn" className="shrink-0">Not hotspot</Badge> : <Badge variant="outline" className="shrink-0">None</Badge>}
            </div>
            <div className={cn("flex items-center justify-between rounded-md border border-border bg-background px-2.5 py-1.5 font-mono text-[12.5px]", status.running && "border-live/40 glow-live")}>
              <span className={cn(status.running ? "text-live" : "text-foreground")}>
                {displayIp ?? "—"}
                <span className="text-muted-foreground">:{displayPort}</span>
              </span>
              <CopyBtn value={url} label="Copy address" />
            </div>
            <select
              className="mt-1.5 h-7 w-full rounded-md border border-border bg-background px-2 text-[12px] text-foreground outline-none focus:ring-1 focus:ring-ring disabled:opacity-50"
              value={selectedIp ?? ""}
              disabled={status.running || busy}
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
          </div>
        </div>
      </Section>

      <Section title="Scan with phone" icon={<Smartphone />}>
        <div className="flex flex-col items-center gap-2">
          {canJoinQr && (
            <div className="grid w-full grid-cols-2 rounded-md border border-border bg-background p-0.5 text-[11px]">
              {(["wifi", "page"] as const).map((t) => (
                <button key={t} className={cn("rounded px-2 py-1 whitespace-nowrap transition-colors", tab === t ? "bg-accent text-foreground" : "text-muted-foreground hover:text-foreground")} onClick={() => setQrTab(t)}>
                  {t === "wifi" ? "1 · Join Wi-Fi" : "2 · Open page"}
                </button>
              ))}
            </div>
          )}
          <div className={cn("rounded-md border border-border bg-background p-2.5 transition-opacity", tab === "page" && !status.running && "opacity-40", tab === "wifi" && !hotspotOn && "opacity-40")}>
            <QRCodeSVG value={tab === "wifi" && hotspot.ssid ? (hotspot.wifiQr ?? wifiQr(hotspot.ssid, hotspot.passphrase)) : (url ?? "http://0.0.0.0:8080")} size={124} bgColor="transparent" fgColor="#fafafa" level="M" />
          </div>
          {tab === "wifi" ? (
            <p className="text-center text-[11px] leading-snug text-muted-foreground">
              Scan with the phone camera to join <span className="text-foreground">{hotspot.ssid}</span>
              {hotspotOn ? "." : " once the hotspot is on."}
            </p>
          ) : (
            <>
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
            </>
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
