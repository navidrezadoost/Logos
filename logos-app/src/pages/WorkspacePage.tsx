import React, { useEffect, useState } from "react";
import { Link, useParams, useSearchParams } from "react-router-dom";
import { ImportMigrationDialog } from "../components/ui/ImportMigrationDialog";
import { Canvas } from "../components/canvas/Canvas";
import { Toolbar } from "../components/toolbar/Toolbar";
import { LayersPanel } from "../components/layers/LayersPanel";
import { Inspector } from "../components/inspector/Inspector";
import { AssetsPanel } from "../components/assets/AssetsPanel";
import { AIPanel } from "../components/ai/AIPanel";
import { TemplateGallery } from "../components/gallery/TemplateGallery";
import { PrototypePreview } from "../components/prototype/PrototypePreview";
import { DevModePanel } from "../components/devmode/DevModePanel";
import { useTemplateStore } from "../stores/templateStore";
import { useProtoStore } from "../stores/prototypeStore";
import { useUiStore } from "../stores/uiStore";
import { useDocumentStore } from "../stores/documentStore";
import { initPersistence, loadPersistedDocument, stopPersistence } from "../offline/persist";
import { createSyncManager } from "../offline/sync";
import { SyncIndicator, useSyncStatus } from "../offline/indicator";
import { theme } from "../theme/colors";

export function WorkspacePage(): React.ReactElement {
  const { projectId, fileId } = useParams<{ projectId: string; fileId: string }>();
  const [searchParams] = useSearchParams();
  const pageIdFromUrl = searchParams.get("page-id");

  const documentId = fileId ?? "local";
  const layersPanelOpen = useUiStore((s) => s.layersPanelOpen);
  const inspectorOpen = useUiStore((s) => s.inspectorOpen);
  const aiPanelOpen = useUiStore((s) => s.aiPanelOpen);
  const toggleAiPanel = useUiStore((s) => s.toggleAiPanel);
  const openGallery = useTemplateStore((s) => s.openGallery);
  const setCurrentPage = useDocumentStore((s) => s.setCurrentPage);
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

    loadPersistedDocument(documentId)
      .then(() => {
        if (pageIdFromUrl && useDocumentStore.getState().pages[pageIdFromUrl]) {
          setCurrentPage(pageIdFromUrl);
        }
        initPersistence(documentId, setSyncStatus);
        mgr = createSyncManager(documentId, setSyncStatus);
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
  }, [documentId, pageIdFromUrl, setCurrentPage, setSyncStatus]);

  if (!projectId || !fileId) {
    return (
      <div style={{ padding: 24, color: theme.text }}>
        Invalid workspace URL. <Link to="/">Back to projects</Link>
      </div>
    );
  }

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "row",
        width: "100vw",
        height: "100vh",
        overflow: "hidden",
        background: theme.panel,
        color: theme.text,
        fontFamily: "'Inter', system-ui, sans-serif",
        position: "relative",
      }}
    >
      <Toolbar />

      {layersPanelOpen && <LayersPanel />}

      <AssetsPanel />

      <Canvas />

      {inspectorOpen && activeTool !== "dev" && <Inspector />}

      {activeTool === "dev" && <DevModePanel />}

      {aiPanelOpen && <AIPanel />}

      <TemplateGallery />

      <PrototypePreview />

      <ImportMigrationDialog
        open={importDialogOpen}
        onClose={() => setImportDialogOpen(false)}
      />

      <Link
        to="/"
        title="Back to projects"
        style={{
          position: "absolute",
          top: 12,
          left: 12,
          zIndex: 200,
          background: theme.surface,
          color: theme.text,
          border: "none",
          borderRadius: 6,
          padding: "6px 10px",
          fontSize: 12,
          fontWeight: 600,
          textDecoration: "none",
        }}
      >
        ← Projects
      </Link>

      <button
        onClick={() => setImportDialogOpen(true)}
        title="Import design tokens from Figma, Sketch, or XD"
        style={{
          position: "absolute",
          top: 12,
          left: 390,
          zIndex: 200,
          background: theme.surface,
          color: theme.accent,
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

      <button
        onClick={openGallery}
        title="Open Template Library"
        style={{
          position: "absolute",
          top: 12,
          left: 110,
          zIndex: 200,
          background: theme.surface,
          color: theme.text,
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

      <button
        onClick={() => setTool(activeTool === "prototype" ? "select" : "prototype")}
        title="Prototype mode — draw connections between frames"
        style={{
          position: "absolute",
          top: 12,
          left: 218,
          zIndex: 200,
          background: activeTool === "prototype" ? theme.accent : theme.surface,
          color: activeTool === "prototype" ? theme.onAccent : theme.text,
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

      <button
        onClick={() => {
          if (previewStartId) startPreview(previewStartId);
        }}
        title="Preview prototype"
        style={{
          position: "absolute",
          top: 12,
          left: 330,
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

      <button
        onClick={toggleAiPanel}
        title="Toggle AI Assistant"
        style={{
          position: "absolute",
          top: 12,
          right: aiPanelOpen ? 316 : 12,
          zIndex: 200,
          background: aiPanelOpen ? theme.accent : theme.surface,
          color: aiPanelOpen ? theme.onAccent : theme.text,
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
