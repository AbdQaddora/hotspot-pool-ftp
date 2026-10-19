import { Activity, FolderOpen, Settings, Share2 } from "lucide-react";
import { cn } from "@/lib/utils";
import type { ServerStatus, StatsEvent } from "@/lib/tauri";

export type Tab = "share" | "pool" | "activity" | "settings";

const ITEMS: Array<{ id: Tab; label: string; icon: React.ComponentType<{ className?: string }>; hint: string }> = [
  { id: "share", label: "Share", icon: Share2, hint: "Connect a phone" },
  { id: "pool", label: "Pool", icon: FolderOpen, hint: "Files & folders" },
  { id: "activity", label: "Activity", icon: Activity, hint: "Devices & log" },
  { id: "settings", label: "Settings", icon: Settings, hint: "Name, hotspot, network" },
];

export function Nav({ tab, onTab, status, stats, poolCount }: { tab: Tab; onTab: (t: Tab) => void; status: ServerStatus; stats: StatsEvent; poolCount: number }) {
  const badge = (id: Tab) => (id === "pool" ? poolCount : id === "activity" && status.running ? stats.connected : 0);
  return (
    <nav className="flex w-[196px] shrink-0 flex-col border-r border-border bg-background">
      <div className="flex-1 p-2">
        {ITEMS.map(({ id, label, icon: Icon, hint }) => {
          const active = tab === id;
          const n = badge(id);
          return (
            <button
              key={id}
              onClick={() => onTab(id)}
              className={cn(
                "group mb-0.5 flex w-full items-center gap-2.5 rounded-md px-2.5 py-2 text-left transition-colors",
                active ? "bg-accent text-foreground" : "text-muted-foreground hover:bg-accent/50 hover:text-foreground",
              )}
            >
              <Icon className={cn("size-4 shrink-0", active ? "text-live" : "text-muted-foreground group-hover:text-foreground")} />
              <span className="min-w-0 flex-1">
                <span className="block text-[13px] font-medium leading-4">{label}</span>
                <span className="block truncate text-[11px] leading-4 text-muted-foreground">{hint}</span>
              </span>
              {n > 0 && <span className="rounded-full bg-secondary px-1.5 py-0.5 font-mono text-[10.5px] leading-none text-muted-foreground">{n}</span>}
            </button>
          );
        })}
      </div>
      <div className="border-t border-border p-3">
        <div className="flex items-center gap-2">
          <span className="relative flex h-2 w-2">
            {status.running && <span className="ping absolute inset-0 rounded-full" />}
            <span className={cn("relative inline-flex h-2 w-2 rounded-full", status.running ? "bg-live" : "bg-muted-foreground/40")} />
          </span>
          <span className="text-[12px] font-medium">{status.running ? "Sharing" : "Not sharing"}</span>
        </div>
        <div className="mt-1 truncate font-mono text-[11px] text-muted-foreground">{status.running ? (status.nameResolves ? status.nameUrl : status.url) : "Flip the switch to start"}</div>
      </div>
    </nav>
  );
}
