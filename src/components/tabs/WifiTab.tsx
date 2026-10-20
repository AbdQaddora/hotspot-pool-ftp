import { QRCodeSVG } from "qrcode.react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { openUrl } from "@tauri-apps/plugin-opener";
import { AlertTriangle, Cable, Check, CheckCircle2, ClipboardList, Copy, ExternalLink, Eye, EyeOff, Globe, Loader2, LogIn, Power, RefreshCw, RotateCcw, Smartphone, Wifi, Wrench } from "lucide-react";
import { useEffect, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardBody, CardHeader, PageHeader, Row } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { cn } from "@/lib/utils";
import { api, wifiQr, type HotspotInfo, type LogEvent, type Settings, type TtlState, type Upstream, type WifiHealth, type WifiShareStatus } from "@/lib/tauri";
import type { useWifiShare } from "@/hooks/useWifiShare";

interface Props {
  wifi: ReturnType<typeof useWifiShare>;
  hotspot: HotspotInfo;
  settings: Settings;
  busy: boolean;
  logs: LogEvent[];
  onToggleHotspot: () => void;
  onRestartHotspot: () => void;
  onSaveSettings: (s: Settings) => Promise<boolean>;
  onOpenHotspotSettings: () => void;
}

const input = "h-8 rounded-md border border-border bg-background px-2.5 text-[12.5px] outline-none focus:ring-1 focus:ring-ring disabled:opacity-50";

const KIND_LABEL: Record<Upstream["kind"], string> = { wifi: "Wi-Fi", ethernet: "Ethernet", cellular: "Cellular", other: "Network" };
const CONN_LABEL: Record<Upstream["connectivity"], string> = { internet: "Internet", signin: "Sign-in needed", local: "Local only", none: "No network" };

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

type Shown = WifiHealth | "checking";

/** Copy for each health state. `wrong_upstream` names what Windows shares vs what was chosen. */
export function healthCopy(health: Shown, s: WifiShareStatus | null): { title: string; detail: string; tone: "live" | "warn" | "muted" } {
  const upstream = s?.upstreams.find((u) => u.id === s.effectiveUpstream)?.name ?? "the chosen connection";
  const shared = s?.sharing.connections.find((c) => c.id === s.sharing.publicId)?.name ?? "another connection";
  switch (health) {
    case "sharing":
      return { title: "Devices get the laptop's internet", detail: `Windows shares ${upstream} with everyone on the hotspot.`, tone: "live" };
    case "not_shared":
      return { title: "Windows is not sharing the internet to the hotspot", detail: "The hotspot is on, but nothing routes the laptop's connection to it. Fix sharing wires it up.", tone: "warn" };
    case "wrong_upstream":
      return { title: `Windows is sharing ${shared} instead of ${upstream}`, detail: "Fix sharing switches it to the connection you picked.", tone: "warn" };
    case "signin":
      return { title: "Sign in on the laptop first", detail: `${upstream} is waiting for its sign-in page. Devices get internet once the laptop is signed in.`, tone: "warn" };
    case "no_internet":
      return { title: "The laptop has no internet to share", detail: "Connect the laptop to Wi-Fi or Ethernet with internet, then come back.", tone: "muted" };
    case "hotspot_off":
      return { title: "Turn on the hotspot", detail: "Devices join the hotspot and use the laptop's connection.", tone: "muted" };
    case "checking":
      return { title: "Checking internet sharing…", detail: "Asking Windows which connection reaches the hotspot.", tone: "muted" };
    case "ap_failed":
      return { title: "The access point could not start; running on Windows Mobile Hotspot", detail: `${s?.apError ?? "Unknown reason."} Mobile Hotspot cannot share the laptop's internet. Fix the cause, then retry the access point.`, tone: "warn" };
    default:
      return { title: "Could not read the sharing state", detail: s?.sharing.error ?? "Windows did not answer. Fix sharing sets it up from scratch.", tone: "warn" };
  }
}

export function WifiTab({ wifi, hotspot, settings, busy, logs, onToggleHotspot, onRestartHotspot, onSaveSettings, onOpenHotspotSettings }: Props) {
  const [showPass, setShowPass] = useState(false);
  const [copiedDiag, setCopiedDiag] = useState(false);
  const [ttl, setTtl] = useState<TtlState | null>(null);
  const [replyTtl, setReplyTtl] = useState<number | null>(null);
  useEffect(() => {
    let alive = true;
    api.getTtlState().then((t) => {
      if (!alive) return;
      setTtl(t);
      // Only measure the reply TTL on builds that can act on it.
      if (t.available) api.detectTtl().then((n) => alive && setReplyTtl(n)).catch(() => {});
    }).catch(() => {});
    return () => {
      alive = false;
    };
  }, [wifi.status?.running]);
  const ttlBlocked = replyTtl === 1;
  const copyDiagnostics = async () => {
    let text: string;
    try {
      text = await api.getDiagnostics();
    } catch (e) {
      text = `diagnostics failed: ${String(e)}`;
    }
    const tail = logs.slice(-40).map((l) => `${new Date(l.ts).toLocaleTimeString()} ${l.level.toUpperCase()} ${l.kind} ${l.message}`).join("\n");
    await writeText(`${text}\nactivity (last ${Math.min(40, logs.length)}):\n${tail}\n`);
    setCopiedDiag(true);
    setTimeout(() => setCopiedDiag(false), 1500);
  };
  const s = wifi.status;
  const hotspotOn = hotspot.state === "on";
  // A snapshot taken before the last hotspot change is stale; say so instead of showing the old verdict.
  const fresh = s !== null && s.hotspot.state === hotspot.state;
  const health: Shown = fresh ? s.health : hotspotOn ? "checking" : "hotspot_off";
  const copy = healthCopy(health, s);
  const canFix = hotspotOn && (health === "not_shared" || health === "wrong_upstream" || health === "unknown");
  const upstreams = s?.upstreams ?? [];
  const selectValue = settings.upstreamAdapter && upstreams.some((u) => u.id === settings.upstreamAdapter) ? settings.upstreamAdapter : "";
  const effective = upstreams.find((u) => u.id === s?.effectiveUpstream);

  return (
    <div className="mx-auto w-full max-w-[880px]">
      <PageHeader
        title="Share Wi-Fi"
        description="Give every device on the hotspot the laptop's internet, even when the network only admits the laptop."
        right={
          <>
            <Button variant="ghost" size="sm" onClick={copyDiagnostics} aria-label="Copy diagnostics">
              {copiedDiag ? <Check className="text-live" /> : <ClipboardList />} {copiedDiag ? "Copied" : "Copy diagnostics"}
            </Button>
            <Button variant="ghost" size="sm" onClick={wifi.refresh} aria-label="Refresh">
              <RefreshCw /> Refresh
            </Button>
          </>
        }
      />

      {/* Hero: hotspot switch + health */}
      <Card className={cn("mb-4 overflow-hidden", health === "sharing" && "border-live/30")}>
        <div className="flex flex-col gap-5 p-5 md:flex-row md:items-center">
          <div className="flex min-w-0 flex-1 items-center gap-4">
            <div className={cn("flex h-12 w-12 shrink-0 items-center justify-center rounded-xl border", health === "sharing" ? "border-live/40 bg-live/10 text-live glow-live" : copy.tone === "warn" ? "border-warn/40 bg-warn/10 text-warn" : "border-border bg-secondary text-muted-foreground")}>
              <Globe className="size-5" />
            </div>
            <div className="min-w-0">
              <div className="text-[15px] font-semibold tracking-tight">{copy.title}</div>
              <div className="mt-0.5 text-[12.5px] text-muted-foreground">{copy.detail}</div>
            </div>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            {health === "signin" && s && (
              <Button variant="outline" className="h-10 px-4 text-[13px]" onClick={() => openUrl(s.signInUrl)}>
                <LogIn /> Open sign-in page
              </Button>
            )}
            {health === "ap_failed" && (
              <Button variant="live" className="h-10 px-4 text-[13px]" onClick={onRestartHotspot} disabled={busy}>
                {busy ? <Loader2 className="animate-spin" /> : <RotateCcw />} Retry access point
              </Button>
            )}
            {canFix && (
              <Button variant="live" className="h-10 px-4 text-[13px]" onClick={wifi.applySharing} disabled={wifi.busy || busy}>
                {wifi.busy ? <Loader2 className="animate-spin" /> : <Wrench />} Fix sharing (admin)
              </Button>
            )}
            {hotspot.supported ? (
              <Button variant={hotspotOn ? "outline" : "live"} className="h-10 px-5 text-[13px]" onClick={onToggleHotspot} disabled={busy}>
                {busy ? <Loader2 className="animate-spin" /> : <Power />}
                {hotspotOn ? "Turn off hotspot" : "Turn on hotspot"}
              </Button>
            ) : (
              <Button variant="outline" className="h-10 px-5 text-[13px]" onClick={onOpenHotspotSettings}>
                <ExternalLink /> Hotspot settings
              </Button>
            )}
          </div>
        </div>
        {hotspot.supported && hotspot.error && !hotspotOn && (
          <div className="flex items-center gap-2 border-t border-warn/30 bg-warn/10 px-5 py-2 text-[12px] text-warn">
            <AlertTriangle className="size-3.5 shrink-0" /> {hotspot.error}
          </div>
        )}
        <div className="flex flex-wrap items-center gap-x-4 gap-y-2 border-t border-border bg-background/60 px-5 py-2.5 text-[12px]">
          <span className="flex items-center gap-1.5 text-muted-foreground">
            Hotspot
            {hotspot.supported ? (
              <Badge variant={hotspotOn ? "live" : "outline"}>{hotspotOn ? "On" : hotspot.state === "transition" ? "Switching…" : "Off"}</Badge>
            ) : (
              <Badge variant="outline">Manual</Badge>
            )}
          </span>
          <span className="flex items-center gap-1.5 text-muted-foreground">
            Sharing
            {!s || !fresh ? (
              <Badge variant="outline"><Loader2 className="size-3 animate-spin" /> checking</Badge>
            ) : health === "sharing" ? (
              <Badge variant="live"><CheckCircle2 className="size-3" /> {effective?.name ?? "on"}</Badge>
            ) : s.sharing.available && s.sharing.publicId ? (
              <Badge variant="warn"><AlertTriangle className="size-3" /> {s.sharing.connections.find((c) => c.id === s.sharing.publicId)?.name ?? "other connection"}</Badge>
            ) : (
              <Badge variant="outline">off</Badge>
            )}
          </span>
          <span className="flex items-center gap-1.5 text-muted-foreground">
            Devices <Badge variant="outline">{s?.clients.length ?? hotspot.clientCount ?? 0}</Badge>
          </span>
          <span className="ml-auto flex items-center gap-1 text-muted-foreground">
            Gateway <span className="font-mono text-foreground">{s?.hotspotAdapter?.ip ?? "—"}</span>
          </span>
        </div>
      </Card>

      <div className="grid gap-4 md:grid-cols-2">
        {/* Upstream */}
        <Card>
          <CardHeader title="Internet to share" description="The laptop connection devices will use." icon={<Cable />} />
          <CardBody>
            <Row label="Hotspot engine" description={settings.hotspotEngine === "mobile" ? "Windows Mobile Hotspot; devices may get no internet." : "Own access point + Windows sharing, the MyPublicWiFi way."}>
              <select className={`${input} max-w-[220px]`} value={settings.hotspotEngine} disabled={busy || hotspotOn} aria-label="Hotspot engine" onChange={(e) => onSaveSettings({ ...settings, hotspotEngine: e.target.value as Settings["hotspotEngine"] })}>
                <option value="wifidirect">Access point + sharing</option>
                <option value="mobile">Windows Mobile Hotspot</option>
              </select>
            </Row>
            <Row label="Connection" description={effective ? `${KIND_LABEL[effective.kind]} · ${CONN_LABEL[effective.connectivity]}` : "No connected network found."}>
              <select
                className={`${input} max-w-[220px]`}
                value={selectValue}
                disabled={busy}
                aria-label="Connection to share"
                onChange={(e) => onSaveSettings({ ...settings, upstreamAdapter: e.target.value || null })}
              >
                <option value="">Automatic{upstreams.find((u) => u.isDefault) ? ` (${upstreams.find((u) => u.isDefault)!.name})` : ""}</option>
                {upstreams.map((u) => (
                  <option key={u.id} value={u.id}>
                    {u.name} · {KIND_LABEL[u.kind]} · {CONN_LABEL[u.connectivity]}
                  </option>
                ))}
              </select>
            </Row>
            <Row label="Hotspot adapter" description="The private side of the share, created by Windows when the hotspot starts.">
              <span className="truncate font-mono text-[12px] text-muted-foreground">{s?.hotspotAdapter?.name ?? "—"}</span>
            </Row>
            {ttl?.available && (
              <Row label="Get past sharing blocks (TTL)" description={ttlBlocked ? "This network forces TTL to 1 to block sharing. Turn this on so devices get internet." : "For networks that block sharing by forcing TTL to 1. Needs one admin prompt; leave off otherwise."}>
                {ttlBlocked && !settings.normalizeTtl && <Badge variant="warn"><AlertTriangle className="size-3" /> blocked</Badge>}
                {ttl.running && <Badge variant="live">on</Badge>}
                <Switch checked={settings.normalizeTtl} onCheckedChange={(v) => onSaveSettings({ ...settings, normalizeTtl: v })} disabled={busy} aria-label="Normalize TTL" />
              </Row>
            )}
            <p className="mt-3 text-[12px] leading-snug text-muted-foreground">Devices appear to the network as the laptop, so a sign-in that only admits one device covers all of them.</p>
          </CardBody>
        </Card>

        {/* Join */}
        <Card className={cn(!hotspotOn && hotspot.supported && "opacity-60")}>
          <CardHeader title="Join the hotspot" description="Scan with the phone camera to connect." icon={<Wifi />} />
          <CardBody className="flex gap-4">
            <div className="shrink-0 rounded-lg border border-border bg-background p-2.5">
              <QRCodeSVG value={hotspot.ssid ? (hotspot.wifiQr ?? wifiQr(hotspot.ssid, hotspot.passphrase)) : "WIFI:T:nopass;S:hotspot;;"} size={120} bgColor="transparent" fgColor="#fafafa" level="M" />
            </div>
            <div className="min-w-0 flex-1 space-y-2 text-[12.5px]">
              <div>
                <div className="text-[11px] uppercase tracking-wider text-muted-foreground">Network</div>
                <div className="truncate font-medium">{hotspot.ssid ?? "—"}</div>
              </div>
              <div>
                <div className="text-[11px] uppercase tracking-wider text-muted-foreground">Password</div>
                <div className="flex items-center gap-1">
                  <span className="truncate font-mono">{hotspot.passphrase ? (showPass ? hotspot.passphrase : "•".repeat(Math.min(hotspot.passphrase.length, 12))) : "none"}</span>
                  <Button variant="ghost" size="xs" onClick={() => setShowPass((v) => !v)} disabled={!hotspot.passphrase} aria-label="Show password">{showPass ? <EyeOff /> : <Eye />}</Button>
                  <CopyBtn value={hotspot.passphrase} label="Copy password" />
                </div>
              </div>
            </div>
          </CardBody>
        </Card>
      </div>

      {/* Devices */}
      <Card className="mt-4">
        <CardHeader title="Connected devices" description="Everyone on the hotspot right now." icon={<Smartphone />} right={<Badge variant="outline">{s?.clients.length ?? 0}</Badge>} />
        <CardBody className="p-0">
          {(s?.clients.length ?? 0) === 0 ? (
            <div className="px-5 py-6 text-center text-[12.5px] text-muted-foreground">{hotspotOn ? "No devices yet. Share the QR code above." : "Turn on the hotspot to see devices."}</div>
          ) : (
            <ul className="divide-y divide-border">
              {s!.clients.map((c) => (
                <li key={c.mac} className="flex items-center gap-3 px-5 py-2.5 text-[12.5px]">
                  <Smartphone className="size-4 text-muted-foreground" />
                  <span className="min-w-0 flex-1 truncate font-medium">{c.name ?? c.ip ?? "Device"}</span>
                  {c.name && c.ip && <span className="font-mono text-[11.5px] text-muted-foreground">{c.ip}</span>}
                  <span className="font-mono text-[11.5px] text-muted-foreground">{c.mac}</span>
                </li>
              ))}
            </ul>
          )}
        </CardBody>
      </Card>
    </div>
  );
}
