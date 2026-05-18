import { Canvas } from "./components/canvas/Canvas";

export default function App(): React.ReactElement {
  return (
    <div
      style={{
        minHeight: "100vh",
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: "24px",
        padding: "40px",
      }}
    >
      <h1
        style={{
          fontSize: "20px",
          fontWeight: 600,
          letterSpacing: "0.05em",
          color: "#cba6f7",
        }}
      >
        Logos — Phase M1 React Shell
      </h1>

      <p
        style={{
          fontSize: "13px",
          color: "#a6adc8",
          maxWidth: "600px",
          textAlign: "center",
          lineHeight: "1.6",
        }}
      >
        The canvas below proves the WASM bridge.{" "}
        <strong>Green badge</strong> = Rust/Skia renderer active.{" "}
        <strong>Red badge</strong> = Canvas 2D fallback (build{" "}
        <code>render-wasm</code> with EMSDK to activate Skia).
      </p>

      <Canvas />

      {/* M2 placeholder — Zustand sidebar + tool palette will be added here */}
      <p style={{ fontSize: "11px", color: "#585b70", fontFamily: "monospace" }}>
        M2 → Zustand state · sidebar · tool palette
      </p>
    </div>
  );
}
