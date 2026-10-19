import { useCallback, useState } from "react";
import { TooltipProvider } from "@/components/ui/tooltip";
import { TopBar } from "@/components/TopBar";
import { Nav, type Tab } from "@/components/Nav";
import { PoolManager } from "@/components/PoolManager";
import { StatusBar } from "@/components/StatusBar";
import { ShareTab } from "@/components/tabs/ShareTab";
import { WifiTab } from "@/components/tabs/WifiTab";
import { ActivityTab } from "@/components/tabs/ActivityTab";
import { SettingsTab } from "@/components/tabs/SettingsTab";
import { InboxTab } from "@/components/tabs/InboxTab";
import { useUploads } from "@/hooks/useUploads";
import { useServer } from "@/hooks/useServer";
import { useWifiShare } from "@/hooks/useWifiShare";
import { usePool } from "@/hooks/usePool";
import { useDragDrop } from "@/hooks/useDragDrop";

export default function App() {
  const server = useServer();
  const pool = usePool();
  const uploads = useUploads();
  const [tab, setTab] = useState<Tab>(() => {
    const t = new URLSearchParams(window.location.search).get("tab");
    return t === "pool" || t === "activity" || t === "settings" || t === "inbox" || t === "wifi" ? t : "share";
  });
  const wifi = useWifiShare({ enabled: tab === "wifi", revision: server.hotspot.state });
  const onDrop = useCallback(
    (paths: string[]) => {
      setTab("pool");
      return pool.addPaths(paths);
    },
    [pool.addPaths],
  );
  const dragging = useDragDrop(onDrop);
  const error = pool.error ?? server.error ?? uploads.error ?? wifi.error;
  const dismiss = () => {
    pool.dismissError();
    server.dismissError();
    uploads.dismissError();
    wifi.dismissError();
  };

  return (
    <TooltipProvider delayDuration={300}>
      <div className="flex h-full flex-col overflow-hidden rounded-[10px] border border-border bg-background">
        <TopBar status={server.status} busy={server.busy} phase={server.phase} onToggle={server.toggle} />
        <div className="flex min-h-0 flex-1">
          <Nav tab={tab} onTab={setTab} status={server.status} stats={server.stats} poolCount={pool.items.length} inboxCount={uploads.items.length} />
          <div className="relative min-w-0 flex-1 overflow-hidden">
            {tab === "pool" ? (
              <PoolManager items={pool.items} status={server.status} dragging={dragging} error={error} onAddFiles={pool.addFiles} onAddFolders={pool.addFolders} onRemove={pool.remove} onSetPassword={pool.setPassword} onDismissError={dismiss} />
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
                {tab === "wifi" && (
                  <WifiTab wifi={wifi} hotspot={server.hotspot} settings={server.settings} busy={server.busy} onToggleHotspot={server.toggleHotspot} onSaveSettings={server.saveSettings} onOpenHotspotSettings={server.openHotspotSettings} />
                )}
                {tab === "inbox" && (
                  <InboxTab items={uploads.items} info={uploads.info} busy={uploads.busy} settings={server.settings} onApprove={uploads.approve} onReject={uploads.reject} onApproveAll={uploads.approveAll} onRejectAll={uploads.rejectAll} onGoSettings={() => setTab("settings")} />
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
                    inbox={uploads.info}
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
