import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter } from "react-router-dom";
import App from "./App";

const root = document.getElementById("root");
if (!root) throw new Error("Missing #root element");

// workspace editor — redirect before React auth boot (avoids stuck "Loading…").
if (/^#\/workspace(\/|\?)/.test(window.location.hash)) {
  window.location.replace("/workspace.html" + window.location.hash);
} else {
  // React dashboard — do not serve workspace.html on reload of /.
  document.cookie = "logos-workspace-shell=; path=/; max-age=0";
  createRoot(root).render(
    <StrictMode>
      <BrowserRouter>
        <App />
      </BrowserRouter>
    </StrictMode>
  );
}
