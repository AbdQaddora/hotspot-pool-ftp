import { QRCodeSVG } from "qrcode.react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { openUrl } from "@tauri-apps/plugin-opener";
import { AlertTriangle, Check, CheckCircle2, Copy, ExternalLink, Eye, EyeOff, Gauge, Globe, Loader2, Pencil, Power, RefreshCw, Settings2, Smartphone, Wifi, WifiOff } from "lucide-react";
import { useEffect, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Sparkline } from "@/components/Sparkline";
import { formatBytes, formatSpeed } from "@/lib/format";
import { cn } from "@/lib/utils";
import { DEFAULT_PORT, SUFFIX_PICKS, shareUrl, wifiQr, type HotspotInfo, type ServerStatus, type Settings, type NetInterface, type StatsEvent } from "@/lib/tauri";

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
  settings: Settings;
  onSaveSettings: (s: Settings) => Promise<boolean>;
  onSetupName: () => void;
  onCheckName: () => void;
  nameBusy: boolean;
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

function ShareNameCard({ status, settings, onSave, onSetup, onCheck, nameBusy, busy }: { status: ServerStatus; settings: Settings; onSave: Props["onSaveSettings"]; onSetup: () => void; onCheck: () => void; nameBusy: boolean; busy: boolean }) {
  const [label, setLabel] = useState(settings.name);
  const [suffix, setSuffix] = useState(settings.suffix);
  useEffect(() => setLabel(settings.name), [settings.name]);
  useEffect(() => setSuffix(settings.suffix), [settings.suffix]);
  const cleanLabel = label.trim().toLowerCase();
  const cleanSuffix = suffix.trim().toLowerCase().replace(/^\./, "");
  const dirty = cleanLabel !== settings.name || cleanSuffix !== settings.suffix;
  const link = !dirty && status.nameUrl ? status.nameUrl : `http://${cleanLabel || "name"}.${cleanSuffix || "link"}`;
  const knownSuffix = SUFFIX_PICKS.includes(cleanSuffix);
  const state = !status.running ? "idle" : nameBusy ? "checking" : status.nameResolves === true ? "ok" : status.nameResolves === false ? "bad" : "unknown";
  return (
    <div className="space-y-2">
      <form
        className="space-y-1.5"
        onSubmit={async (e) => {
          e.preventDefault();
          await onSave({ ...settings, name: cleanLabel, suffix: cleanSuffix });
        }}
      >
        <div className="flex items-center gap-1">
          <div className="flex h-7 min-w-0 flex-1 items-center rounded-md border border-border bg-background pr-1 focus-within:ring-1 focus-within:ring-ring">
            <span className="shrink-0 pl-2 font-mono text-[11px] text-muted-foreground">http://</span>
            <input size={1} className="w-full min-w-0 flex-1 bg-transparent px-0.5 font-mono text-[12px] outline-none" value={label} onChange={(e) => setLabel(e.target.value)} maxLength={32} spellCheck={false} aria-label="Share name" />
            <span className="shrink-0 font-mono text-[11px] text-muted-foreground">.</span>
            <input size={1} className="w-12 min-w-0 bg-transparent px-0.5 font-mono text-[12px] outline-none" value={suffix} onChange={(e) => setSuffix(e.target.value)} maxLength={24} spellCheck={false} aria-label="Ending" list="suffix-picks" />
            <datalist id="suffix-picks">
              {SUFFIX_PICKS.map((s) => (
                <option key={s} value={s} />
              ))}
            </datalist>
          </div>
          <Button type="submit" size="sm" className="shrink-0" variant={dirty ? "default" : "outline"} disabled={!dirty || busy}>
            Save
          </Button>
        </div>
        {!knownSuffix && cleanSuffix && (
          <p className="text-[11px] leading-snug text-warn">
            Phones only open typed names that end in a real domain. Use one of {SUFFIX_PICKS.map((s) => `.${s}`).join(", ")}, or add a slash when typing (<span className="font-mono">{cleanLabel || "name"}.{cleanSuffix}/</span>).
          </p>
        )}
      </form>
      <div className={cn("flex items-center justify-between gap-2 rounded-md border px-2.5 py-1.5 font-mono text-[12.5px]", state === "ok" ? "border-live/40 glow-live text-live" : "border-border text-foreground")}>
        <span className="truncate" title={link}>{link}</span>
        <span className="flex items-center gap-0.5">
          {state === "ok" && <CheckCircle2 className="size-3.5 text-live" />}
          {state === "bad" && <AlertTriangle className="size-3.5 text-warn" />}
          {state === "checking" && <Loader2 className="size-3.5 animate-spin text-muted-foreground" />}
          <CopyBtn value={status.running ? link : null} label="Copy link" />
          <Button variant="ghost" size="xs" onClick={() => openUrl(link)} disabled={!status.running} aria-label="Open link">
            <ExternalLink />
          </Button>
        </span>
      </div>
      {state === "ok" && <p className="text-[11px] leading-snug text-muted-foreground">Phones on the hotspot can type <span className="font-mono text-foreground">{status.name}</span> in the browser and it opens directly.</p>}
      {state === "bad" && (
        <div className="space-y-1.5">
          <p className="text-[11px] leading-snug text-warn">The hotspot DNS does not answer this name yet. Phones can use the IP link{status.hostnameUrl ? ` or ${status.hostnameUrl}` : ""}.</p>
          <div className="flex gap-1.5">
            <Button size="sm" variant="outline" className="flex-1" onClick={onSetup} disabled={nameBusy}>
              <Settings2 /> Set up name (admin)
            </Button>
            <Button size="sm" variant="ghost" onClick={onCheck} disabled={nameBusy} aria-label="Check again">
              <RefreshCw />
            </Button>
          </div>
        </div>
      )}
      {state === "idle" && <p className="text-[11px] leading-snug text-muted-foreground">Start sharing to activate the name. Windows asks for permission once to add it to the hosts file.</p>}
    </div>
  );
}

export function Sidebar({ status, stats, history, interfaces, selectedIp, onSelectIp, onRefresh, hotspot, busy, settings, onSaveSettings, onSetupName, onCheckName, nameBusy, onToggleHotspot, onConfigureHotspot, onOpenHotspotSettings }: Props) {
  const [qrTab, setQrTab] = useState<"wifi" | "page">("wifi");
  const autoHotspot = settings.autoHotspot;
  const onAutoHotspot = (v: boolean) => onSaveSettings({ ...settings, autoHotspot: v });
  const pageUrl = status.running && status.nameResolves ? status.nameUrl : null;
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
                {displayPort !== 80 && <span className="text-muted-foreground">:{displayPort}</span>}
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

      <Section title="Share name" icon={<Globe />}>
        <ShareNameCard status={status} settings={settings} onSave={onSaveSettings} onSetup={onSetupName} onCheck={onCheckName} nameBusy={nameBusy} busy={busy} />
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
            <QRCodeSVG value={tab === "wifi" && hotspot.ssid ? (hotspot.wifiQr ?? wifiQr(hotspot.ssid, hotspot.passphrase)) : (pageUrl ?? url ?? "http://0.0.0.0:8080")} size={124} bgColor="transparent" fgColor="#fafafa" level="M" />
          </div>
          {tab === "wifi" ? (
            <p className="text-center text-[11px] leading-snug text-muted-foreground">
              Scan with the phone camera to join <span className="text-foreground">{hotspot.ssid}</span>
              {hotspotOn ? "." : " once the hotspot is on."}
            </p>
          ) : (
            <>
              <code className="w-full truncate text-center font-mono text-[10.5px] text-muted-foreground" title={pageUrl ?? url ?? ""}>
                {pageUrl ?? url ?? "server offline"}
              </code>
              {status.running ? (
                <Button variant="outline" size="sm" className="w-full" onClick={() => openUrl(pageUrl ?? url ?? "")}>
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
