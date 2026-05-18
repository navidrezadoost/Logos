import { Canvas } from "./components/canvas/Canvas";
import { Toolbar } from "./components/toolbar/Toolbar";
import { LayersPanel } from "./components/layers/LayersPanel";
import { Inspector } from "./components/inspector/Inspector";
import { useUiStore } from "./stores/uiStore";

export default function App(): React.ReactElement {
  const layersPanelOpen = useUiStore((s) => s.layersPanelOpen);
  const inspectorOpen = useUiStore((s) => s.inspectorOpen);

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
      }}
    >
      {/* Left tool palette */}
      <Toolbar />

      {/* Layers panel */}
      {layersPanelOpen && <LayersPanel />}

      {/* Main canvas — fills remaining space */}
      <Canvas />

      {/* Right inspector panel */}
      {inspectorOpen && <Inspector />}
    </div>
  );
}
