import { useCallback, useState } from "react";
import { TooltipProvider } from "@/components/ui/tooltip";
import { TopBar } from "@/components/TopBar";
import { Nav, type Tab } from "@/components/Nav";
import { PoolManager } from "@/components/PoolManager";
import { StatusBar } from "@/components/StatusBar";
import { ShareTab } from "@/components/tabs/ShareTab";
import { ActivityTab } from "@/components/tabs/ActivityTab";
import { SettingsTab } from "@/components/tabs/SettingsTab";
import { useServer } from "@/hooks/useServer";
import { usePool } from "@/hooks/usePool";
import { useDragDrop } from "@/hooks/useDragDrop";

export default function App() {
  const server = useServer();
  const pool = usePool();
  const [tab, setTab] = useState<Tab>(() => {
    const t = new URLSearchParams(window.location.search).get("tab");
    return t === "pool" || t === "activity" || t === "settings" ? t : "share";
  });
  const onDrop = useCallback(
    (paths: string[]) => {
      setTab("pool");
      return pool.addPaths(paths);
    },
    [pool.addPaths],
  );
  const dragging = useDragDrop(onDrop);
  const error = pool.error ?? server.error;
  const dismiss = () => {
    pool.dismissError();
    server.dismissError();
  };

  return (
    <TooltipProvider delayDuration={300}>
      <div className="flex h-full flex-col overflow-hidden rounded-[10px] border border-border bg-background">
        <TopBar status={server.status} busy={server.busy} phase={server.phase} onToggle={server.toggle} />
        <div className="flex min-h-0 flex-1">
          <Nav tab={tab} onTab={setTab} status={server.status} stats={server.stats} poolCount={pool.items.length} />
          <div className="relative min-w-0 flex-1 overflow-hidden">
            {tab === "pool" ? (
              <PoolManager items={pool.items} status={server.status} dragging={dragging} error={error} onAddFiles={pool.addFiles} onAddFolders={pool.addFolders} onRemove={pool.remove} onDismissError={dismiss} />
            ) : (
              <div className="h-full overflow-y-auto p-6">
                {error && (
                  <div className="mx-auto mb-4 flex w-full max-w-[880px] items-start gap-2 rounded-lg border border-warn/30 bg-warn/10 px-3 py-2 text-[12px] text-warn">
                    <pre className="flex-1 whitespace-pre-wrap font-sans">{error}</pre>
                    <button className="text-warn/80 hover:text-warn" onClick={dismiss} aria-label="Dismiss">✕</button>
                  </div>
                )}
                {tab === "share" && (
                  <ShareTab
                    status={server.status}
                    hotspot={server.hotspot}
                    settings={server.settings}
                    busy={server.busy}
                    phase={server.phase}
                    nameBusy={server.nameBusy}
                    onToggle={server.toggle}
                    onSetupName={server.setupName}
                    onCheckName={server.checkName}
                    onGoSettings={() => setTab("settings")}
                  />
                )}
                {tab === "activity" && <ActivityTab status={server.status} stats={server.stats} history={server.history} logs={server.logs} onClear={server.clearLogs} />}
                {tab === "settings" && (
                  <SettingsTab
                    status={server.status}
                    settings={server.settings}
                    hotspot={server.hotspot}
                    interfaces={server.interfaces}
                    selectedIp={server.selectedIp}
                    busy={server.busy}
                    nameBusy={server.nameBusy}
                    onSaveSettings={server.saveSettings}
                    onSetupName={server.setupName}
                    onCheckName={server.checkName}
                    onToggleHotspot={server.toggleHotspot}
                    onConfigureHotspot={server.configureHotspot}
                    onOpenHotspotSettings={server.openHotspotSettings}
                    onSelectIp={server.setSelectedIp}
                    onRefresh={() => {
                      server.refreshInterfaces();
                      server.refreshHotspot();
                    }}
                  />
                )}
              </div>
            )}
            {dragging && tab !== "pool" && (
              <div className="pointer-events-none absolute inset-0 z-20 flex items-center justify-center bg-background/70 backdrop-blur-[2px]">
                <div className="rounded-xl border-2 border-dashed border-live/60 px-8 py-6 text-[14px] font-medium text-live">Drop to add to the pool</div>
              </div>
            )}
          </div>
        </div>
        <StatusBar status={server.status} totalSize={pool.totalSize} pending={pool.pending} itemCount={pool.items.length} last={server.logs[server.logs.length - 1]} />
      </div>
    </TooltipProvider>
  );
}
