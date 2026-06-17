/**
 * Expose the shared workspace $APP namespace for classic (non-module) Logos scripts.
 * Pan/viewport helpers live on $APP after main.js initializes the store.
 */
import { $APP } from "./shared.js";

globalThis.$APP = $APP;

function logosWorkspaceBridgeStatus() {
  return {
    hasAPP: !!$APP,
    hasStoreEmit: !!$APP.$app$main$store$emit_BANG_$$,
    hasKeyboardEvent: !!$APP.$app$util$keyboard$KeyboardEvent$$,
    hasViewportPan: !!$APP.$app$main$data$workspace$viewport$update_viewport_position$$,
    hasPersistentArrayMap: !!$APP.$cljs$core$PersistentArrayMap$$,
    hasKeywordX: !!$APP.$cljs$cst$1500$x$$,
    hasKeywordY: !!$APP.$cljs$cst$1501$y$$,
  };
}

globalThis.logosWorkspaceBridgeStatus = logosWorkspaceBridgeStatus;
console.info("[logos-workspace-bridge] loaded", logosWorkspaceBridgeStatus());

globalThis.logosWorkspaceReady = new Promise(function (resolve) {
  var attempts = 0;
  function tick() {
    attempts++;
    if (
      $APP.$app$main$store$emit_BANG_$$ &&
      $APP.$app$main$data$workspace$viewport$update_viewport_position$$
    ) {
      console.info("[logos-workspace-bridge] ready", logosWorkspaceBridgeStatus());
      resolve($APP);
      return;
    }
    if (attempts > 6000) {
      console.warn("[logos-workspace-bridge] ready timeout", logosWorkspaceBridgeStatus());
      resolve($APP);
      return;
    }
    requestAnimationFrame(tick);
  }
  tick();
});

export { $APP };
