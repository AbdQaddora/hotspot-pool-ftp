import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { AlertTriangle, Check, FolderOpen, Inbox, ShieldAlert, ShieldCheck, Smartphone, Trash2, X } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardBody, CardHeader, PageHeader } from "@/components/ui/card";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { formatBytes, formatTime } from "@/lib/format";
import { cn } from "@/lib/utils";
import type { InboxInfo, Settings, UploadItem } from "@/lib/tauri";

interface Props {
  items: UploadItem[];
  info: InboxInfo | null;
  busy: string | null;
  settings: Settings;
  onApprove: (id: string) => void;
  onReject: (id: string) => void;
  onApproveAll: () => void;
  onRejectAll: () => void;
  onGoSettings: () => void;
}

function Row({ item, busy, onApprove, onReject }: { item: UploadItem; busy: string | null; onApprove: () => void; onReject: () => void }) {
  const disabled = busy === item.id || busy === "all";
  return (
    <div className={cn("flex items-center gap-3 border-b border-border px-5 py-3 last:border-b-0", item.executable && "bg-warn/5")}>
      <div className={cn("flex h-9 w-9 shrink-0 items-center justify-center rounded-lg border", item.executable ? "border-warn/40 bg-warn/10 text-warn" : "border-border bg-secondary text-muted-foreground")}>
        {item.executable ? <ShieldAlert className="size-4" /> : <Smartphone className="size-4" />}
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="truncate text-[13px] font-medium" title={item.originalName}>{item.name}</span>
          {item.executable && <Badge variant="warn">Executable</Badge>}
          {item.originalName !== item.name && (
            <Tooltip>
              <TooltipTrigger asChild><Badge variant="outline">renamed</Badge></TooltipTrigger>
              <TooltipContent>Sent as “{item.originalName}”; unsafe characters were replaced.</TooltipContent>
            </Tooltip>
          )}
        </div>
        <div className="mt-0.5 flex flex-wrap gap-x-3 text-[11.5px] text-muted-foreground">
          <span className="font-mono tabular-nums">{formatBytes(item.size)}</span>
          <span>from <span className="text-foreground">{item.device || "unnamed phone"}</span> · {item.ip}</span>
          <span>{formatTime(item.receivedAt)}</span>
          <span className="font-mono" title={item.sha256}>sha256 {item.sha256.slice(0, 10)}…</span>
        </div>
      </div>
      <div className="flex shrink-0 items-center gap-1">
        <Tooltip>
          <TooltipTrigger asChild>
            <Button variant="ghost" size="xs" onClick={() => revealItemInDir(item.path)} aria-label="Reveal in Explorer"><FolderOpen /></Button>
          </TooltipTrigger>
          <TooltipContent>Reveal in Explorer / Finder</TooltipContent>
        </Tooltip>
        <Button variant="destructive" size="sm" onClick={onReject} disabled={disabled}><X /> Reject</Button>
        <Button variant="live" size="sm" onClick={onApprove} disabled={disabled}><Check /> Approve</Button>
      </div>
    </div>
  );
}

export function InboxTab({ items, info, busy, settings, onApprove, onReject, onApproveAll, onRejectAll, onGoSettings }: Props) {
  const pct = info && info.quotaBytes ? Math.min(100, (info.usedBytes / info.quotaBytes) * 100) : 0;
  return (
    <div className="mx-auto w-full max-w-[880px] space-y-4">
      <PageHeader
        title="Inbox"
        description="Files phones sent to this laptop. Nothing here is shared until you approve it."
        right={
          <>
            <Button variant="outline" size="sm" onClick={onRejectAll} disabled={!items.length || busy === "all"}><Trash2 /> Reject all</Button>
            <Button variant="live" size="sm" onClick={onApproveAll} disabled={!items.length || busy === "all"}><ShieldCheck /> Approve all</Button>
          </>
        }
      />

      <Card>
        <CardBody className="flex flex-wrap items-center gap-x-5 gap-y-2 py-3 text-[12px] text-muted-foreground">
          <span className="flex items-center gap-1.5">
            Uploads {settings.allowUploads ? <Badge variant="live">On</Badge> : <Badge variant="outline">Off</Badge>}
          </span>
          <span className="flex items-center gap-1.5">PIN {settings.uploadPin ? <Badge variant="live">Required</Badge> : <Badge variant="warn">None</Badge>}</span>
          <span>Max file <span className="font-mono text-foreground">{formatBytes(settings.maxUploadMb * 1024 * 1024, 0)}</span></span>
          <span className="flex min-w-[160px] flex-1 items-center gap-2">
            Inbox
            <span className="h-1.5 flex-1 overflow-hidden rounded-full bg-secondary"><span className={cn("block h-full rounded-full", pct > 85 ? "bg-warn" : "bg-live")} style={{ width: `${pct}%` }} /></span>
            <span className="font-mono text-foreground">{info ? `${formatBytes(info.usedBytes)} / ${formatBytes(info.quotaBytes, 0)}` : "—"}</span>
          </span>
          <button className="text-[12px] underline-offset-2 hover:underline" onClick={onGoSettings}>Upload settings</button>
        </CardBody>
      </Card>

      {!settings.uploadPin && settings.allowUploads && (
        <div className="flex items-start gap-2 rounded-lg border border-warn/30 bg-warn/10 px-3 py-2 text-[12px] text-warn">
          <AlertTriangle className="mt-0.5 size-3.5 shrink-0" />
          <span>Anyone on the hotspot can send files. Set an upload PIN in Settings if you share the hotspot with people you don't know.</span>
        </div>
      )}

      <Card>
        <CardHeader title={items.length ? `${items.length} waiting for review` : "Nothing waiting"} description="Approve moves a file into the shared “Uploads” folder in the pool. Reject deletes it." icon={<Inbox />} />
        {items.length === 0 ? (
          <CardBody className="py-10 text-center text-[12.5px] text-muted-foreground">Phones can send files from the <span className="text-foreground">Upload</span> tab on the page. They land here first.</CardBody>
        ) : (
          <div>
            {items.map((it) => (
              <Row key={it.id} item={it} busy={busy} onApprove={() => onApprove(it.id)} onReject={() => onReject(it.id)} />
            ))}
          </div>
        )}
      </Card>
    </div>
  );
}
