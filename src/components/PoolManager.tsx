import { useState } from "react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { AlertTriangle, Check, File, FileArchive, FileAudio, FileImage, FileText, FileVideo, Folder, FolderOpen, FolderPlus, FilePlus, Link2, Trash2, UploadCloud, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { formatBytes } from "@/lib/format";
import { ftpLink, type FtpStatus, type PoolItem } from "@/lib/tauri";
import { cn } from "@/lib/utils";

interface Props {
  items: PoolItem[];
  status: FtpStatus;
  dragging: boolean;
  error: string | null;
  onAddFiles: () => void;
  onAddFolders: () => void;
  onRemove: (id: string) => void;
  onDismissError: () => void;
}

const EXT_ICON: Array<[RegExp, React.ComponentType<{ className?: string }>]> = [
  [/\.(mp4|mkv|mov|avi|webm|m4v)$/i, FileVideo],
  [/\.(mp3|flac|wav|aac|ogg|m4a)$/i, FileAudio],
  [/\.(png|jpe?g|gif|webp|svg|heic|bmp)$/i, FileImage],
  [/\.(zip|rar|7z|tar|gz|iso)$/i, FileArchive],
  [/\.(txt|md|pdf|docx?|xlsx?|pptx?|csv|json)$/i, FileText],
];

function iconFor(item: PoolItem) {
  if (item.isDir) return Folder;
  return EXT_ICON.find(([re]) => re.test(item.name))?.[1] ?? File;
}

function IconBtn({ label, onClick, disabled, children, danger }: { label: string; onClick: () => void; disabled?: boolean; danger?: boolean; children: React.ReactNode }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant={danger ? "destructive" : "ghost"} size="xs" onClick={onClick} disabled={disabled} aria-label={label}>
          {children}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

function Row({ item, status, onRemove }: { item: PoolItem; status: FtpStatus; onRemove: (id: string) => void }) {
  const Icon = iconFor(item);
  const link = ftpLink(status, item);
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    if (!link) return;
    await writeText(link);
    setCopied(true);
    setTimeout(() => setCopied(false), 1200);
  };
  return (
    <tr className="group border-b border-border/70 transition-colors hover:bg-accent/40">
      <td className="w-9 pl-3">
        <Icon className={cn("size-4", item.isDir ? "text-warn" : "text-muted-foreground")} />
      </td>
      <td className="max-w-0 py-2 pr-3">
        <div className="truncate text-[12.5px] font-medium text-foreground" title={item.name}>
          {item.name}
        </div>
        <div className="truncate font-mono text-[10.5px] text-muted-foreground" title={item.path}>
          {item.path}
        </div>
      </td>
      <td className="w-24 whitespace-nowrap pr-3 text-right font-mono text-[11.5px] tabular-nums text-muted-foreground">
        {item.sizePending ? <span className="animate-pulse">calculating…</span> : formatBytes(item.size)}
      </td>
      <td className="w-28 pr-2">
        <div className="flex items-center justify-end gap-0.5 opacity-0 transition-opacity group-hover:opacity-100 focus-within:opacity-100">
          <IconBtn label={link ? "Copy FTP link" : "Start server to get a link"} onClick={copy} disabled={!link}>
            {copied ? <Check className="text-live" /> : <Link2 />}
          </IconBtn>
          <IconBtn label="Open in Explorer / Finder" onClick={() => revealItemInDir(item.path)}>
            <FolderOpen />
          </IconBtn>
          <IconBtn label="Remove from pool" onClick={() => onRemove(item.id)} danger>
            <Trash2 />
          </IconBtn>
        </div>
      </td>
    </tr>
  );
}

export function PoolManager({ items, status, dragging, error, onAddFiles, onAddFolders, onRemove, onDismissError }: Props) {
  const folders = items.filter((i) => i.isDir).length;
  const files = items.length - folders;

  return (
    <main className="relative flex min-w-0 flex-1 flex-col bg-background">
      <div className="flex h-12 shrink-0 items-center gap-2 border-b border-border px-4">
        <h2 className="text-[13px] font-semibold tracking-tight">Shared Pool</h2>
        <Badge variant="mono">
          {folders} folders · {files} files
        </Badge>
        <div className="flex-1" />
        <Button variant="outline" size="sm" onClick={onAddFolders}>
          <FolderPlus /> Add Folders
        </Button>
        <Button variant="default" size="sm" onClick={onAddFiles}>
          <FilePlus /> Add Files
        </Button>
      </div>

      {error && (
        <div className="flex items-start gap-2 border-b border-warn/30 bg-warn/10 px-4 py-2 text-[12px] text-warn">
          <AlertTriangle className="mt-0.5 size-3.5 shrink-0" />
          <pre className="flex-1 whitespace-pre-wrap font-sans">{error}</pre>
          <Button variant="ghost" size="xs" onClick={onDismissError} aria-label="Dismiss">
            <X />
          </Button>
        </div>
      )}

      <div className="relative min-h-0 flex-1 overflow-auto">
        {items.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-3 p-8 text-center">
            <div className="flex h-12 w-12 items-center justify-center rounded-xl border border-dashed border-border bg-card">
              <UploadCloud className="size-5 text-muted-foreground" />
            </div>
            <div>
              <p className="text-[13px] font-medium">Your pool is empty</p>
              <p className="mt-1 max-w-xs text-[12px] text-muted-foreground">
                Drop files or folders anywhere in this window, or use the buttons above. Everything you add appears under <code className="font-mono text-foreground">/Pool</code> on the phone.
              </p>
            </div>
          </div>
        ) : (
          <table className="w-full border-collapse">
            <thead className="sticky top-0 z-10 bg-background/95 backdrop-blur">
              <tr className="border-b border-border text-left text-[10.5px] uppercase tracking-wider text-muted-foreground">
                <th className="py-2 pl-3" />
                <th className="py-2 pr-3 font-medium">Name / Path</th>
                <th className="py-2 pr-3 text-right font-medium">Size</th>
                <th className="py-2 pr-2" />
              </tr>
            </thead>
            <tbody>
              {items.map((it) => (
                <Row key={it.id} item={it} status={status} onRemove={onRemove} />
              ))}
            </tbody>
          </table>
        )}
      </div>

      <div
        className={cn(
          "pointer-events-none absolute inset-0 z-20 flex items-center justify-center transition-opacity duration-150",
          dragging ? "opacity-100" : "opacity-0",
        )}
      >
        <div className="drag-overlay absolute inset-0 bg-background/70 backdrop-blur-[2px]" />
        <div className="relative m-4 flex h-[calc(100%-2rem)] w-[calc(100%-2rem)] flex-col items-center justify-center gap-2 rounded-xl border-2 border-dashed border-live/60">
          <UploadCloud className="size-7 text-live" />
          <p className="text-[14px] font-medium text-live">Drop to add to the pool</p>
          <p className="text-[12px] text-muted-foreground">Files and folders are shared instantly</p>
        </div>
      </div>
    </main>
  );
}
