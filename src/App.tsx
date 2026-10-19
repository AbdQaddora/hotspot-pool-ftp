import { useCallback } from "react";
import { TooltipProvider } from "@/components/ui/tooltip";
import { TopBar } from "@/components/TopBar";
import { Sidebar } from "@/components/Sidebar";
import { PoolManager } from "@/components/PoolManager";
import { StatusBar } from "@/components/StatusBar";
import { useServer } from "@/hooks/useServer";
import { usePool } from "@/hooks/usePool";
import { useDragDrop } from "@/hooks/useDragDrop";

export default function App() {
  const server = useServer();
  const pool = usePool();
  const onDrop = useCallback((paths: string[]) => pool.addPaths(paths), [pool.addPaths]);
  const dragging = useDragDrop(onDrop);

  return (
    <TooltipProvider delayDuration={300}>
      <div className="flex h-full flex-col overflow-hidden rounded-[10px] border border-border bg-background">
        <TopBar status={server.status} busy={server.busy} phase={server.phase} onToggle={server.toggle} />
        <div className="flex min-h-0 flex-1">
          <Sidebar
            status={server.status}
            stats={server.stats}
            history={server.history}
            interfaces={server.interfaces}
            selectedIp={server.selectedIp}
            onSelectIp={server.setSelectedIp}
            onRefresh={() => {
              server.refreshInterfaces();
              server.refreshHotspot();
            }}
            hotspot={server.hotspot}
            busy={server.busy}
            autoHotspot={server.autoHotspot}
            onAutoHotspot={server.setAutoHotspot}
            onToggleHotspot={server.toggleHotspot}
            onConfigureHotspot={server.configureHotspot}
            onOpenHotspotSettings={server.openHotspotSettings}
          />
          <PoolManager
            items={pool.items}
            status={server.status}
            dragging={dragging}
            error={pool.error ?? server.error}
            onAddFiles={pool.addFiles}
            onAddFolders={pool.addFolders}
            onRemove={pool.remove}
            onDismissError={() => {
              pool.dismissError();
              server.dismissError();
            }}
          />
        </div>
        <StatusBar status={server.status} totalSize={pool.totalSize} pending={pool.pending} itemCount={pool.items.length} logs={server.logs} />
      </div>
    </TooltipProvider>
  );
}
