import { QRCodeSVG } from "qrcode.react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { openUrl } from "@tauri-apps/plugin-opener";
import { AlertTriangle, Check, CheckCircle2, Copy, ExternalLink, Eye, EyeOff, Globe, Loader2, Power, RefreshCw, Smartphone, Wifi } from "lucide-react";
import { useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardBody, CardHeader, PageHeader } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { cn } from "@/lib/utils";
import { wifiQr, type HotspotInfo, type ServerStatus, type Settings } from "@/lib/tauri";

interface Props {
  status: ServerStatus;
  hotspot: HotspotInfo;
  settings: Settings;
  busy: boolean;
  phase: "idle" | "hotspot" | "server" | "stopping";
  nameBusy: boolean;
  onToggle: () => void;
  onSetupName: () => void;
  onCheckName: () => void;
  onGoSettings: () => void;
  onSaveSettings: (s: Settings) => Promise<boolean>;
}

function CopyBtn({ value, label, size = "xs" }: { value: string | null; label: string; size?: "xs" | "sm" }) {
  const [copied, setCopied] = useState(false);
  return (
    <Button
      variant="ghost"
      size={size}
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

export function ShareTab({ status, hotspot, settings, busy, phase, nameBusy, onToggle, onSetupName, onCheckName, onGoSettings, onSaveSettings }: Props) {
  const [showPass, setShowPass] = useState(false);
  const hotspotOn = hotspot.state === "on";
  const nameOk = status.running && status.nameResolves === true;
  const primary = nameOk ? status.nameUrl : status.url;
  const typed = nameOk ? status.name : status.ip;
  const pageQr = primary ?? `http://${settings.name}.${settings.suffix}`;
  const phaseText = { idle: "", hotspot: "Turning on the hotspot…", server: "Starting the server…", stopping: "Stopping…" }[phase];

  return (
    <div className="mx-auto w-full max-w-[880px]">
      <PageHeader title="Share" description="Everything a phone needs to connect and download." />

      {/* Hero */}
      <Card className={cn("mb-4 overflow-hidden", status.running && "border-live/30")}>
        <div className="flex flex-col gap-5 p-5 md:flex-row md:items-center">
          <div className="flex min-w-0 flex-1 items-center gap-4">
            <div className={cn("flex h-12 w-12 shrink-0 items-center justify-center rounded-xl border", status.running ? "border-live/40 bg-live/10 text-live glow-live" : "border-border bg-secondary text-muted-foreground")}>
              <Wifi className="size-5" />
            </div>
            <div className="min-w-0">
              <div className="text-[15px] font-semibold tracking-tight">{busy ? phaseText : status.running ? "Sharing is on" : "Not sharing"}</div>
              <div className="mt-0.5 text-[12.5px] text-muted-foreground">
                {status.running ? (
                  <>
                    Phones on <span className="font-medium text-foreground">{hotspot.ssid ?? "the hotspot"}</span> can type <span className="font-mono text-foreground">{typed}</span> in their browser.
                  </>
                ) : hotspot.supported ? (
                  "The switch turns on the hotspot, starts the server and sets up the name."
                ) : (
                  "Turn on your hotspot, then flip the switch."
                )}
              </div>
            </div>
          </div>
          <Button size="default" variant={status.running ? "outline" : "live"} className="h-10 shrink-0 px-5 text-[13px]" onClick={onToggle} disabled={busy}>
            {busy ? <Loader2 className="animate-spin" /> : <Power />}
            {status.running ? "Stop sharing" : "Start sharing"}
          </Button>
        </div>
        <div className="flex flex-wrap items-center gap-x-4 gap-y-2 border-t border-border bg-background/60 px-5 py-2.5 text-[12px]">
          <span className="flex items-center gap-1.5 text-muted-foreground">
            Hotspot
            {hotspot.supported ? (
              <Badge variant={hotspotOn ? "live" : "outline"}>{hotspotOn ? `On${hotspot.clientCount != null ? ` · ${hotspot.clientCount} devices` : ""}` : hotspot.state === "transition" ? "Switching…" : "Off"}</Badge>
            ) : (
              <Badge variant="outline">Manual</Badge>
            )}
          </span>
          <span className="flex items-center gap-1.5 text-muted-foreground">
            Name
            {!status.running ? (
              <Badge variant="outline">{settings.name}.{settings.suffix}</Badge>
            ) : nameBusy ? (
              <Badge variant="outline"><Loader2 className="size-3 animate-spin" /> checking</Badge>
            ) : status.nameResolves ? (
              <Badge variant="live"><CheckCircle2 className="size-3" /> {status.name}</Badge>
            ) : (
              <Badge variant="warn"><AlertTriangle className="size-3" /> {status.name} not resolving</Badge>
            )}
          </span>
          {status.running && status.nameResolves === false && (
            <span className="flex items-center gap-1">
              <Button size="sm" variant="outline" onClick={onSetupName} disabled={nameBusy}>Set up name (admin)</Button>
              <Button size="xs" variant="ghost" onClick={onCheckName} disabled={nameBusy} aria-label="Check again"><RefreshCw /></Button>
            </span>
          )}
          <span className="flex items-center gap-2 text-muted-foreground">
            <Globe className="size-3.5" /> Share internet
            <Switch checked={settings.shareInternet} onCheckedChange={(v) => onSaveSettings({ ...settings, shareInternet: v })} disabled={busy || status.running} aria-label="Share internet with devices" />
            {!settings.shareInternet && <Badge variant="outline">Files only</Badge>}
          </span>
          <span className="ml-auto flex items-center gap-1 text-muted-foreground">
            IP <span className="font-mono text-foreground">{status.ip ?? "—"}</span>
            <CopyBtn value={status.url} label="Copy IP link" />
          </span>
        </div>
      </Card>

      {/* Link */}
      <Card className="mb-4">
        <CardBody className="flex items-center gap-3">
          <div className="min-w-0 flex-1">
            <div className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">Link for the phone</div>
            <div className={cn("mt-1 truncate font-mono text-[22px] font-semibold tracking-tight", status.running ? "text-live" : "text-muted-foreground")}>{primary ?? `http://${settings.name}.${settings.suffix}`}</div>
            {status.running && !nameOk && <div className="mt-1 text-[12px] text-muted-foreground">Showing the IP until the name resolves on the hotspot.</div>}
          </div>
          <CopyBtn value={primary} label="Copy link" size="sm" />
          <Button variant="outline" size="sm" onClick={() => primary && openUrl(primary)} disabled={!primary}>
            <ExternalLink /> Open
          </Button>
        </CardBody>
      </Card>

      {/* Steps */}
      <div className="grid gap-4 md:grid-cols-2">
        <Card className={cn(!hotspotOn && hotspot.supported && "opacity-60")}>
          <CardHeader title="1 · Join the Wi-Fi" description="Scan with the phone camera to connect to the hotspot." icon={<Wifi />} />
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
              {!hotspot.supported && <button className="text-[12px] text-muted-foreground underline-offset-2 hover:underline" onClick={onGoSettings}>Hotspot is manual on this OS · settings</button>}
            </div>
          </CardBody>
        </Card>
        <Card className={cn(!status.running && "opacity-60")}>
          <CardHeader title="2 · Open the page" description="Scan, or type the name in the browser." icon={<Smartphone />} />
          <CardBody className="flex gap-4">
            <div className="shrink-0 rounded-lg border border-border bg-background p-2.5">
              <QRCodeSVG value={pageQr} size={120} bgColor="transparent" fgColor="#fafafa" level="M" />
            </div>
            <div className="min-w-0 flex-1 space-y-2 text-[12.5px]">
              <div>
                <div className="text-[11px] uppercase tracking-wider text-muted-foreground">Type this</div>
                <div className="truncate font-mono text-[14px] font-medium">{typed ?? `${settings.name}.${settings.suffix}`}</div>
              </div>
              <p className="text-[12px] leading-snug text-muted-foreground">Tap a file to download it. Folders open in place. Images, video and audio preview inside the page.</p>
            </div>
          </CardBody>
        </Card>
      </div>
    </div>
  );
}
