import React, { useEffect } from "react";
import { Canvas } from "./components/canvas/Canvas";
import { Toolbar } from "./components/toolbar/Toolbar";
import { LayersPanel } from "./components/layers/LayersPanel";
import { Inspector } from "./components/inspector/Inspector";
import { AssetsPanel } from "./components/assets/AssetsPanel";
import { useUiStore } from "./stores/uiStore";
import { initPersistence, loadPersistedDocument, stopPersistence } from "./offline/persist";
import { createSyncManager } from "./offline/sync";
import { SyncIndicator, useSyncStatus } from "./offline/indicator";

const DOCUMENT_ID = "local";

export default function App(): React.ReactElement {
  const layersPanelOpen = useUiStore((s) => s.layersPanelOpen);
  const inspectorOpen = useUiStore((s) => s.inspectorOpen);
  const [syncStatus, setSyncStatus] = useSyncStatus(
    navigator.onLine ? "online" : "offline"
  );

  useEffect(() => {
    let mgr: ReturnType<typeof createSyncManager> | null = null;

    loadPersistedDocument(DOCUMENT_ID)
      .then(() => {
        initPersistence(DOCUMENT_ID, setSyncStatus);
        mgr = createSyncManager(DOCUMENT_ID, setSyncStatus);
        mgr.start();
      })
      .catch((err) => {
        console.error("[logos/app] Failed to load persisted document:", err);
        setSyncStatus("error");
      });

    return () => {
      stopPersistence();
      mgr?.stop();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "row",
        width: "100vw",
        height: "100vh",
        overflow: "hidden",
        background: "#1e1e2e",
        color: "#cdd6f4",
        fontFamily: "'Inter', system-ui, sans-serif",
        position: "relative",
      }}
    >
      {/* Left tool palette */}
      <Toolbar />

      {/* Layers panel */}
      {layersPanelOpen && <LayersPanel />}

      {/* Assets panel (component library) */}
      <AssetsPanel />

      {/* Main canvas — fills remaining space */}
      <Canvas />

      {/* Right inspector panel */}
      {inspectorOpen && <Inspector />}

      {/* Sync status indicator — bottom-right overlay */}
      <div
        style={{
          position: "absolute",
          bottom: 12,
          right: 12,
          zIndex: 100,
          background: "rgba(30,30,46,0.85)",
          backdropFilter: "blur(6px)",
          borderRadius: 6,
          padding: "4px 8px",
          boxShadow: "0 1px 4px rgba(0,0,0,0.4)",
        }}
      >
        <SyncIndicator status={syncStatus} />
      </div>
    </div>
  );
}
