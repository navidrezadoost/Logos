/**
 * Expose Penpot's shared $APP namespace for classic (non-module) Logos scripts.
 * Pan/viewport helpers live on $APP after main.js initializes the store.
 */
import { $APP } from "./shared.js";

globalThis.$APP = $APP;

globalThis.logosPenpotReady = new Promise(function (resolve) {
  var attempts = 0;
  function tick() {
    attempts++;
    if (
      $APP.$app$main$store$emit_BANG_ &&
      $APP.$app$main$data$workspace$viewport$update_viewport_position$$
    ) {
      resolve($APP);
      return;
    }
    if (attempts > 6000) {
      resolve($APP);
      return;
    }
    requestAnimationFrame(tick);
  }
  tick();
});

export { $APP };
