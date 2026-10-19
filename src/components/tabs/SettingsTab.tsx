import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { AlertTriangle, CheckCircle2, ExternalLink, FolderOpen, Globe, Inbox, Info, Loader2, Network, Power, RefreshCw, Settings2, Wifi } from "lucide-react";
import { useEffect, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardBody, CardHeader, PageHeader, Row } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { SUFFIX_PICKS, type HotspotInfo, type InboxInfo, type NetInterface, type ServerStatus, type Settings } from "@/lib/tauri";

interface Props {
  status: ServerStatus;
  settings: Settings;
  hotspot: HotspotInfo;
  interfaces: NetInterface[];
  selectedIp: string | null;
  busy: boolean;
  nameBusy: boolean;
  onSaveSettings: (s: Settings) => Promise<boolean>;
  onSetupName: () => void;
  onCheckName: () => void;
  onToggleHotspot: () => void;
  onConfigureHotspot: (ssid: string, passphrase: string) => Promise<boolean>;
  onOpenHotspotSettings: () => void;
  onSelectIp: (ip: string) => void;
  onRefresh: () => void;
  inbox: InboxInfo | null;
}

const input = "h-8 rounded-md border border-border bg-background px-2.5 text-[12.5px] outline-none focus:ring-1 focus:ring-ring disabled:opacity-50";

export function SettingsTab({ status, settings, hotspot, interfaces, selectedIp, busy, nameBusy, onSaveSettings, onSetupName, onCheckName, onToggleHotspot, onConfigureHotspot, onOpenHotspotSettings, onSelectIp, onRefresh, inbox }: Props) {
  const [pin, setPin] = useState(settings.uploadPin);
  const [maxMb, setMaxMb] = useState(String(settings.maxUploadMb));
  const [quotaMb, setQuotaMb] = useState(String(settings.inboxQuotaMb));
  useEffect(() => setPin(settings.uploadPin), [settings.uploadPin]);
  useEffect(() => setMaxMb(String(settings.maxUploadMb)), [settings.maxUploadMb]);
  useEffect(() => setQuotaMb(String(settings.inboxQuotaMb)), [settings.inboxQuotaMb]);
  const uploadDirty = pin !== settings.uploadPin || Number(maxMb) !== settings.maxUploadMb || Number(quotaMb) !== settings.inboxQuotaMb;
  const [label, setLabel] = useState(settings.name);
  const [suffix, setSuffix] = useState(settings.suffix);
  const [ssid, setSsid] = useState(hotspot.ssid ?? "");
  const [pass, setPass] = useState(hotspot.passphrase ?? "");
  useEffect(() => setLabel(settings.name), [settings.name]);
  useEffect(() => setSuffix(settings.suffix), [settings.suffix]);
  useEffect(() => setSsid(hotspot.ssid ?? ""), [hotspot.ssid]);
  useEffect(() => setPass(hotspot.passphrase ?? ""), [hotspot.passphrase]);

  const cleanLabel = label.trim().toLowerCase();
  const cleanSuffix = suffix.trim().toLowerCase().replace(/^\./, "");
  const nameDirty = cleanLabel !== settings.name || cleanSuffix !== settings.suffix;
  const knownSuffix = SUFFIX_PICKS.includes(cleanSuffix);
  const hotspotOn = hotspot.state === "on";
  const credsDirty = ssid !== (hotspot.ssid ?? "") || pass !== (hotspot.passphrase ?? "");
  const resolveState = !status.running ? "idle" : nameBusy ? "checking" : status.nameResolves === true ? "ok" : status.nameResolves === false ? "bad" : "unknown";

  return (
    <div className="mx-auto w-full max-w-[880px] space-y-4">
      <PageHeader title="Settings" description="Name, hotspot and network. Changes are saved on this laptop." />

      <Card>
        <CardHeader title="Share name" description="What people type in the phone browser." icon={<Globe />} />
        <CardBody>
          <form
            className="space-y-3"
            onSubmit={async (e) => {
              e.preventDefault();
              await onSaveSettings({ ...settings, name: cleanLabel, suffix: cleanSuffix });
            }}
          >
            <Row label="Address" description="A short label plus a real domain ending.">
              <div className="flex h-8 items-center rounded-md border border-border bg-background pr-1 focus-within:ring-1 focus-within:ring-ring">
                <span className="pl-2.5 font-mono text-[12px] text-muted-foreground">http://</span>
                <input size={1} className="w-[120px] bg-transparent px-1 font-mono text-[12.5px] outline-none" value={label} onChange={(e) => setLabel(e.target.value)} maxLength={32} spellCheck={false} aria-label="Label" />
                <span className="font-mono text-[12px] text-muted-foreground">.</span>
                <input size={1} className="w-[64px] bg-transparent px-1 font-mono text-[12.5px] outline-none" value={suffix} onChange={(e) => setSuffix(e.target.value)} maxLength={24} spellCheck={false} aria-label="Ending" list="suffix-picks" />
                <datalist id="suffix-picks">
                  {SUFFIX_PICKS.map((s) => (
                    <option key={s} value={s} />
                  ))}
                </datalist>
              </div>
              <Button type="submit" size="sm" variant={nameDirty ? "default" : "outline"} disabled={!nameDirty || busy}>
                Save
              </Button>
            </Row>
            {!knownSuffix && cleanSuffix && (
              <p className="flex items-start gap-2 rounded-md border border-warn/30 bg-warn/10 px-3 py-2 text-[12px] leading-snug text-warn">
                <AlertTriangle className="mt-0.5 size-3.5 shrink-0" />
                <span>
                  Phones only open typed names that end in a real domain. Pick one of {SUFFIX_PICKS.map((s) => `.${s}`).join(", ")}, or type a slash after the name (<span className="font-mono">{cleanLabel || "name"}.{cleanSuffix}/</span>).
                </span>
              </p>
            )}
            <Row label="Status on the hotspot" description="Checked with a real DNS query each time sharing starts.">
              {resolveState === "idle" && <Badge variant="outline">Not sharing</Badge>}
              {resolveState === "checking" && <Badge variant="outline"><Loader2 className="size-3 animate-spin" /> Checking</Badge>}
              {resolveState === "ok" && <Badge variant="live"><CheckCircle2 className="size-3" /> {status.name} resolves</Badge>}
              {resolveState === "bad" && <Badge variant="warn"><AlertTriangle className="size-3" /> Not resolving</Badge>}
              {status.running && resolveState !== "ok" && (
                <Button size="sm" variant="outline" onClick={onSetupName} disabled={nameBusy}>
                  <Settings2 /> Set up (admin)
                </Button>
              )}
              {status.running && (
                <Button size="xs" variant="ghost" onClick={onCheckName} disabled={nameBusy} aria-label="Check again">
                  <RefreshCw />
                </Button>
              )}
            </Row>
          </form>
        </CardBody>
      </Card>

      <Card>
        <CardHeader
          title="Mobile hotspot"
          description={hotspot.supported ? "Controlled through Windows. No admin rights needed." : (hotspot.error ?? "Not automated on this OS.")}
          icon={<Wifi />}
          right={
            hotspot.supported ? (
              <Badge variant={hotspotOn ? "live" : "outline"}>{hotspotOn ? "On" : hotspot.state === "transition" ? "Switching…" : "Off"}</Badge>
            ) : (
              <Badge variant="outline">Manual</Badge>
            )
          }
        />
        <CardBody>
          {hotspot.supported ? (
            <>
              <Row label="Manage with the Share switch" description="Turn the hotspot on when sharing starts and off again when it stops.">
                <Switch checked={settings.autoHotspot} onCheckedChange={(v) => onSaveSettings({ ...settings, autoHotspot: v })} disabled={busy} />
              </Row>
              <Row label="Hotspot" description={hotspotOn ? `${hotspot.clientCount ?? 0} device(s) connected` : "Currently off"}>
                <Button size="sm" variant={hotspotOn ? "outline" : "live"} onClick={onToggleHotspot} disabled={busy || status.running}>
                  <Power /> {hotspotOn ? "Turn off" : "Turn on"}
                </Button>
                <Button size="sm" variant="ghost" onClick={onOpenHotspotSettings}>
                  <ExternalLink /> Windows settings
                </Button>
              </Row>
              <form
                onSubmit={async (e) => {
                  e.preventDefault();
                  await onConfigureHotspot(ssid, pass);
                }}
              >
                <Row label="Network name & password" description="Change while the hotspot is off. Password 8+ characters.">
                  <input className={`${input} w-[150px]`} value={ssid} onChange={(e) => setSsid(e.target.value)} placeholder="Name" maxLength={32} disabled={hotspotOn || busy} aria-label="Hotspot name" />
                  <input className={`${input} w-[150px] font-mono`} value={pass} onChange={(e) => setPass(e.target.value)} placeholder="Password" minLength={8} maxLength={63} disabled={hotspotOn || busy} aria-label="Hotspot password" />
                  <Button type="submit" size="sm" variant={credsDirty ? "default" : "outline"} disabled={!credsDirty || hotspotOn || busy}>
                    Apply
                  </Button>
                </Row>
              </form>
            </>
          ) : (
            <Row label="Open the system hotspot settings" description="Turn the hotspot on there, then flip the Share switch.">
              <Button size="sm" variant="outline" onClick={onOpenHotspotSettings}>
                <ExternalLink /> Open settings
              </Button>
            </Row>
          )}
        </CardBody>
      </Card>

      <Card>
        <CardHeader
          title="Uploads from phones"
          description="Files land in a quarantine inbox and are shared only after you approve them."
          icon={<Inbox />}
          right={<Switch checked={settings.allowUploads} onCheckedChange={(v) => onSaveSettings({ ...settings, allowUploads: v })} disabled={busy} aria-label="Allow uploads" />}
        />
        <CardBody>
          <form
            onSubmit={async (e) => {
              e.preventDefault();
              await onSaveSettings({ ...settings, uploadPin: pin.trim(), maxUploadMb: Math.max(1, Number(maxMb) || 1), inboxQuotaMb: Math.max(1, Number(quotaMb) || 1) });
            }}
          >
            <Row label="Upload PIN" description="Phones must enter it before sending. Leave empty to allow anyone on the hotspot. 5 wrong tries lock an IP for a minute.">
              <input className={`${input} w-[150px] font-mono`} value={pin} onChange={(e) => setPin(e.target.value)} placeholder="none" maxLength={32} aria-label="Upload PIN" />
            </Row>
            <Row label="Largest file" description="Bigger uploads are refused before any data is received.">
              <input type="number" min={1} className={`${input} w-[110px] text-right font-mono`} value={maxMb} onChange={(e) => setMaxMb(e.target.value)} aria-label="Max upload MB" />
              <span className="text-[12px] text-muted-foreground">MB</span>
            </Row>
            <Row label="Inbox size limit" description="Total space the quarantine may use until you review it.">
              <input type="number" min={1} className={`${input} w-[110px] text-right font-mono`} value={quotaMb} onChange={(e) => setQuotaMb(e.target.value)} aria-label="Inbox quota MB" />
              <span className="text-[12px] text-muted-foreground">MB</span>
              <Button type="submit" size="sm" variant={uploadDirty ? "default" : "outline"} disabled={!uploadDirty || busy}>Save</Button>
            </Row>
          </form>
          <Row label="Inbox folder" description={inbox?.dir ?? "—"}>
            <Button size="sm" variant="ghost" onClick={() => inbox?.dir && revealItemInDir(inbox.dir)} disabled={!inbox?.dir}><FolderOpen /> Open</Button>
            <Button
              size="sm"
              variant="outline"
              disabled={busy}
              onClick={async () => {
                const sel = await openDialog({ directory: true, multiple: false, title: "Choose the inbox folder" });
                if (typeof sel === "string") await onSaveSettings({ ...settings, inboxDir: sel });
              }}
            >
              Change…
            </Button>
            {settings.inboxDir && <Button size="sm" variant="ghost" onClick={() => onSaveSettings({ ...settings, inboxDir: null })}>Default</Button>}
          </Row>
        </CardBody>
      </Card>

      <Card>
        <CardHeader title="Network adapter" description="Which adapter the server binds to. Auto-detected on every start." icon={<Network />} right={<Button variant="ghost" size="xs" onClick={onRefresh} aria-label="Rescan"><RefreshCw /></Button>} />
        <CardBody>
          <Row label="Adapter" description={status.running ? "Stop sharing to change the adapter." : "★ marks a detected hotspot gateway."}>
            <select className={`${input} max-w-[300px]`} value={selectedIp ?? ""} disabled={status.running || busy} onChange={(e) => onSelectIp(e.target.value)}>
              {interfaces.length === 0 && <option value="">No interfaces found</option>}
              {interfaces.map((i) => (
                <option key={`${i.name}-${i.ip}`} value={i.ip}>
                  {i.ip} · {i.name}
                  {i.isHotspot ? " ★" : ""}
                </option>
              ))}
            </select>
          </Row>
        </CardBody>
      </Card>

      <Card>
        <CardHeader title="About" icon={<Info />} />
        <CardBody>
          <Row label={`Hotspot Pool v${__APP_VERSION__}`} description="Share files with phones over your laptop's Wi-Fi hotspot. Nothing leaves the network.">
            <Button size="sm" variant="ghost" onClick={() => openUrl("https://github.com/AbdQaddora/hotspot-pool-ftp")}>
              <ExternalLink /> GitHub
            </Button>
          </Row>
        </CardBody>
      </Card>
    </div>
  );
}
