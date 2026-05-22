import React, { useEffect, useState } from "react";
import { ImportMigrationDialog } from "./components/ui/ImportMigrationDialog";
import { Canvas } from "./components/canvas/Canvas";
import { Toolbar } from "./components/toolbar/Toolbar";
import { LayersPanel } from "./components/layers/LayersPanel";
import { Inspector } from "./components/inspector/Inspector";
import { AssetsPanel } from "./components/assets/AssetsPanel";
import { AIPanel } from "./components/ai/AIPanel";
import { TemplateGallery } from "./components/gallery/TemplateGallery";
import { PrototypePreview } from "./components/prototype/PrototypePreview";
import { DevModePanel } from "./components/devmode/DevModePanel";
import { useTemplateStore } from "./stores/templateStore";
import { useProtoStore } from "./stores/prototypeStore";
import { useUiStore } from "./stores/uiStore";
import { useDocumentStore } from "./stores/documentStore";
import { initPersistence, loadPersistedDocument, stopPersistence } from "./offline/persist";
import { createSyncManager } from "./offline/sync";
import { SyncIndicator, useSyncStatus } from "./offline/indicator";

const DOCUMENT_ID = "local";

export default function App(): React.ReactElement {
  const layersPanelOpen = useUiStore((s) => s.layersPanelOpen);
  const inspectorOpen = useUiStore((s) => s.inspectorOpen);
  const aiPanelOpen = useUiStore((s) => s.aiPanelOpen);
  const toggleAiPanel = useUiStore((s) => s.toggleAiPanel);
  const openGallery = useTemplateStore((s) => s.openGallery);
  const [importDialogOpen, setImportDialogOpen] = useState(false);
  const { activeTool, setTool } = useUiStore();
  const { startPreview } = useProtoStore();
  const currentPageId = useDocumentStore((s) => s.currentPageId);
  const pages = useDocumentStore((s) => s.pages);
  const [syncStatus, setSyncStatus] = useSyncStatus(
    navigator.onLine ? "online" : "offline"
  );

  const currentPage = pages[currentPageId];
  const previewStartId = currentPage?.rootShapeIds[0] ?? null;

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

      {/* Right inspector panel — hidden in Dev mode */}
      {inspectorOpen && activeTool !== "dev" && <Inspector />}

      {/* Dev Mode inspection panel — shown only when Dev tool is active */}
      {activeTool === "dev" && <DevModePanel />}

      {/* AI Design Assistant panel */}
      {aiPanelOpen && <AIPanel />}

      {/* Template Library (always mounted; manages open state internally) */}
      <TemplateGallery />

      {/* Prototype Preview (always mounted; manages open state internally) */}
      <PrototypePreview />

      {/* Import migration dialog */}
      <ImportMigrationDialog
        open={importDialogOpen}
        onClose={() => setImportDialogOpen(false)}
      />

      {/* Import tokens button */}
      <button
        onClick={() => setImportDialogOpen(true)}
        title="Import design tokens from Figma, Sketch, or XD"
        style={{
          position: "absolute",
          top: 12,
          left: 390,
          zIndex: 200,
          background: "#313244",
          color: "#7efff5",
          border: "none",
          borderRadius: 6,
          padding: "6px 10px",
          fontSize: 12,
          fontWeight: 600,
          cursor: "pointer",
        }}
      >
        ↓ Import tokens
      </button>

      {/* Templates button */}
      <button
        onClick={openGallery}
        title="Open Template Library"
        style={{
          position: "absolute",
          top: 12,
          left: 60,
          zIndex: 200,
          background: "#313244",
          color: "#cdd6f4",
          border: "none",
          borderRadius: 6,
          padding: "6px 10px",
          fontSize: 12,
          fontWeight: 600,
          cursor: "pointer",
        }}
      >
        ⊞ Templates
      </button>

      {/* Prototype tool button */}
      <button
        onClick={() => setTool(activeTool === "prototype" ? "select" : "prototype")}
        title="Prototype mode — draw connections between frames"
        style={{
          position: "absolute",
          top: 12,
          left: 168,
          zIndex: 200,
          background: activeTool === "prototype" ? "#cba6f7" : "#313244",
          color: activeTool === "prototype" ? "#1e1e2e" : "#cdd6f4",
          border: "none",
          borderRadius: 6,
          padding: "6px 10px",
          fontSize: 12,
          fontWeight: 600,
          cursor: "pointer",
          transition: "background 0.15s",
        }}
      >
        ⬡ Prototype
      </button>

      {/* Preview button */}
      <button
        onClick={() => {
          if (previewStartId) startPreview(previewStartId);
        }}
        title="Preview prototype"
        style={{
          position: "absolute",
          top: 12,
          left: 280,
          zIndex: 200,
          background: "#313244",
          color: "#a6e3a1",
          border: "none",
          borderRadius: 6,
          padding: "6px 10px",
          fontSize: 12,
          fontWeight: 600,
          cursor: "pointer",
        }}
      >
        ▶ Preview
      </button>

      {/* AI toggle button */}
      <button
        onClick={toggleAiPanel}
        title="Toggle AI Assistant"
        style={{
          position: "absolute",
          top: 12,
          right: aiPanelOpen ? 316 : 12,
          zIndex: 200,
          background: aiPanelOpen ? "#89b4fa" : "#313244",
          color: aiPanelOpen ? "#1e1e2e" : "#cdd6f4",
          border: "none",
          borderRadius: 6,
          padding: "6px 10px",
          fontSize: 12,
          fontWeight: 600,
          cursor: "pointer",
          transition: "right 0.15s, background 0.15s",
        }}
      >
        ✦ AI
      </button>

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
