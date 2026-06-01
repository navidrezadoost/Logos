/**
 * Logos — Move / Hand / Scale (Figma-style tool group on the Move slot).
 *
 * One combo control overlays Penpot's Move button: active tool icon + chevron.
 * Move delegates to Penpot; Hand pans the viewport; Scale enables uniform resize.
 */
(function () {
  "use strict";

  var LOG_PREFIX = "[logos-move-tool]";
  var BUILD = "2026-05-30-figma-combo7";
  var DRAG_THRESHOLD_PX = 3;

  var WORKSPACE_VIEWPORT_SEL = ".main_ui_workspace__workspace-viewport";
  var WORKSPACE_CONTENT_SEL = ".main_ui_workspace__workspace-content";
  var CANVAS_IGNORE_SEL =
    ".main_ui_workspace_top_toolbar__main-toolbar," +
    ".main_ui_workspace_left_header," +
    ".main_ui_workspace_right_header," +
    ".main_ui_workspace_sidebar," +
    ".logos-move-tool-menu," +
    ".logos-toolbar-position-menu," +
    "#modal";

  if (
    localStorage.getItem("logos:workspace:no-toolbar") === "1" ||
    /(?:^|[?&])logos-no-toolbar(?:=1)?(?:&|$)/.test(location.search)
  ) {
    return;
  }

  var DEBUG =
    localStorage.getItem("logos:workspace:debug") === "1" ||
    /(?:^|[?&])logos-debug(?:=1)?(?:&|$)/.test(location.search);

  function handLog() {
    var args = [LOG_PREFIX + " hand +" + (performance.now() | 0) + "ms"];
    for (var i = 0; i < arguments.length; i++) args.push(arguments[i]);
    console.info.apply(console, args);
  }

  function describeTarget(target) {
    if (!target || !target.tagName) return String(target);
    var id = target.id ? "#" + target.id : "";
    var cls =
      typeof target.className === "string" && target.className
        ? "." + target.className.trim().split(/\s+/).slice(0, 3).join(".")
        : "";
    return target.tagName.toLowerCase() + id + cls;
  }

  function log() {
    if (!DEBUG) return;
    handLog.apply(null, arguments);
  }

  function warnOnce(key, msg) {
    if (warnOnce._done && warnOnce._done[key]) return;
    warnOnce._done = warnOnce._done || {};
    warnOnce._done[key] = true;
    console.warn(LOG_PREFIX, msg);
  }

  function error() {
    var args = [LOG_PREFIX + " ERROR"];
    for (var i = 0; i < arguments.length; i++) args.push(arguments[i]);
    console.error.apply(console, args);
  }

  var STORAGE_KEY = "logos:workspace:move-tool";
  var storage = sessionStorage;
  var TOOLS = ["move", "hand", "scale"];
  var DEFAULT_TOOL = "move";

  var TOOLBAR_SEL = ".main_ui_workspace_top_toolbar__main-toolbar";
  var OPTIONS_SEL = ".main_ui_workspace_top_toolbar__main-toolbar-options";
  var BTN_CLASS = "main_ui_workspace_top_toolbar__main-toolbar-options-button";
  var VIEWPORT_SEL =
    ".main_ui_workspace__workspace-viewport, .viewport-controls, .viewport-selrect, #viewport-controls, canvas";

  var LABELS = { move: "Move", hand: "Hand", scale: "Scale" };
  var SHORTCUTS = { move: "V", hand: "H", scale: "K" };

  var openMenuEl = null;
  var menuAnchorRef = null;
  var menuDismissHandler = null;
  var viewportDragBound = false;
  var viewportDragMode = null;
  var lastPanPoint = null;
  var pendingHandDrag = null;
  var handPointerSession = false;
  var spacePanActive = false;
  var toolbarRef = null;
  var moveBtnRef = null;
  var comboRef = null;

  var shortcutsBound = false;
  var initDone = false;
  var initialToolApplied = false;
  var watchedMoveSlots = typeof WeakSet !== "undefined" ? new WeakSet() : null;

  function readTool() {
    try {
      var stored = storage.getItem(STORAGE_KEY);
      return TOOLS.indexOf(stored) >= 0 ? stored : DEFAULT_TOOL;
    } catch (e) {
      return DEFAULT_TOOL;
    }
  }

  function writeTool(tool) {
    try {
      storage.setItem(STORAGE_KEY, tool);
    } catch (e) {
      /* ignore */
    }
  }

  function isPanModeActive() {
    return readTool() === "hand" || spacePanActive;
  }

  function systemIcon(key) {
    var icons = globalThis.LogosSystemIcons;
    if (icons && icons[key]) return icons[key];
    return (
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="16" height="16" ' +
      'class="logos-system-icon" fill="currentColor" aria-hidden="true"><text x="8" y="11" ' +
      'text-anchor="middle" font-size="7">' +
      key.charAt(0).toUpperCase() +
      "</text></svg>"
    );
  }

  function toolIcon(tool) {
    if (tool === "move") {
      return '<svg width="16" height="16" aria-hidden="true" class="icon-move"><use href="#icon-move"></use></svg>';
    }
    return systemIcon(tool);
  }

  function penpotAPP() {
    return globalThis.$APP || null;
  }

  function panningApiReady() {
    var APP = penpotAPP();
    return !!(
      APP &&
      APP.$app$main$store$emit_BANG_ &&
      APP.$app$main$data$workspace$viewport$update_viewport_position$$
    );
  }

  function getViewportZoom() {
    var APP = penpotAPP();
    if (!APP || !APP.$app$main$store$state$$ || !APP.$cljs$core$_deref$$ || !APP.$cljs$core$get_in$$) {
      return 1;
    }
    try {
      var state = APP.$cljs$core$_deref$$(APP.$app$main$store$state$$);
      var zoom = APP.$cljs$core$get_in$$.$cljs$core$IFn$_invoke$arity$2$(
        state,
        new APP.$cljs$core$PersistentVector$(
          null,
          2,
          5,
          APP.$cljs$core$PersistentVector$EMPTY_NODE$$,
          [APP.$cljs$cst$1906$workspace_local$$, APP.$cljs$cst$2159$zoom$$],
          null
        )
      );
      return zoom && zoom > 0 ? zoom : 1;
    } catch (e) {
      return 1;
    }
  }

  function penpotEmit(event) {
    var APP = penpotAPP();
    if (!APP || !APP.$app$main$store$emit_BANG_) return false;
    try {
      APP.$app$main$store$emit_BANG_.$cljs$core$IFn$_invoke$arity$1$(event);
      return true;
    } catch (e) {
      error("penpotEmit failed", e);
      return false;
    }
  }

  function penpotEmitVariadic(event, rest) {
    var APP = penpotAPP();
    if (!APP || !APP.$app$main$store$emit_BANG_) return false;
    try {
      if (rest && rest.length && APP.$cljs$core$prim_seq$cljs$0core$0IFn$0_invoke$0arity$02$$) {
        APP.$app$main$store$emit_BANG_.$cljs$core$IFn$_invoke$arity$variadic$(
          event,
          APP.$cljs$core$prim_seq$cljs$0core$0IFn$0_invoke$0arity$02$$(rest)
        );
      } else {
        APP.$app$main$store$emit_BANG_.$cljs$core$IFn$_invoke$arity$1$(event);
      }
      return true;
    } catch (e) {
      error("penpotEmitVariadic failed", e);
      return false;
    }
  }

  function isScaleTextEnabled() {
    var APP = penpotAPP();
    if (!APP || !APP.$app$main$refs$workspace_layout$$ || !APP.$rumext$v2$deref$$) {
      return false;
    }
    try {
      var layout = APP.$rumext$v2$deref$$(APP.$app$main$refs$workspace_layout$$);
      return APP.$cljs$core$contains_QMARK_$$(
        layout,
        APP.$cljs$cst$3657$scale_text$$
      );
    } catch (e) {
      return false;
    }
  }

  function setScaleTextEnabled(enabled) {
    var APP = penpotAPP();
    if (!APP) return;
    var flag = APP.$cljs$cst$3657$scale_text$$;
    var fn = APP.$app$main$data$workspace$layout$toggle_layout_flag$$;
    if (!flag || typeof fn !== "function") return;

    var currently = isScaleTextEnabled();
    if (enabled === currently) return;

    if (enabled) {
      var forceKey = APP.$cljs$cst$3675$force_QMARK_;
      var opts = new APP.$cljs$core$PersistentArrayMap$(null, 1, [forceKey, true], null);
      penpotEmit(
        fn.$cljs$core$IFn$_invoke$arity$variadic$(
          flag,
          APP.$cljs$core$prim_seq$cljs$0core$0IFn$0_invoke$0arity$02$$([opts])
        )
      );
      log("scale-text flag enabled");
      return;
    }

    penpotEmit(fn(flag));
    log("scale-text flag disabled");
  }

  function findMoveButton(toolbar) {
    if (!toolbar) return null;
    var options = toolbar.querySelector(OPTIONS_SEL);
    if (!options) return null;
    var moveLi = options.querySelector("li:first-child");
    if (moveLi) {
      var nativeBtn = moveLi.querySelector(
        "button:not(.logos-move-tool-combo__main):not(.logos-move-tool-combo__chevron)"
      );
      if (nativeBtn) return nativeBtn;
    }
    var icon = options.querySelector(".icon-move");
    return icon ? icon.closest("button") : options.querySelector("button");
  }

  function cleanupLegacyUi(options) {
    if (!options) return;
    options.querySelectorAll('[data-logos-toolbar="move-tool-group"]').forEach(function (el) {
      el.remove();
    });
  }

  function updateComboDisplay() {
    if (!comboRef) return;
    var tool = readTool();
    var primary = comboRef.querySelector(".logos-move-tool-combo__main");
    if (!primary) return;

    primary.innerHTML = toolIcon(tool);
    primary.title = LABELS[tool] + " (" + SHORTCUTS[tool] + ")";
    primary.setAttribute("aria-label", LABELS[tool]);
    comboRef.setAttribute("data-active-tool", tool);

    var penpotSelected =
      moveBtnRef &&
      moveBtnRef.classList.contains("main_ui_workspace_top_toolbar__selected");
    primary.classList.toggle(
      "logos-move-tool-combo__main--active",
      tool === "hand" || tool === "scale" || (tool === "move" && penpotSelected)
    );
  }

  function emitWorkspaceInterrupt() {
    var fn = globalThis.$app$main$data$workspace$comments$handle_interrupt$$;
    if (typeof fn === "function") {
      penpotEmit(fn());
    }
  }

  function cljsPoint(x, y) {
    var APP = penpotAPP();
    if (!APP || !APP.$app$common$geom$point$point$$) return null;
    return APP.$app$common$geom$point$point$$.$cljs$core$IFn$_invoke$arity$2$(x, y);
  }

  function isHandModeActive() {
    return isPanModeActive();
  }

  function applyToolState(tool) {
    document.body.classList.toggle("logos-move-tool-hand", tool === "hand");
    document.body.classList.toggle("logos-move-tool-scale", tool === "scale");
    updateComboDisplay();
    log("tool state", tool);
  }

  function activateTool(tool, source) {
    log("activateTool", tool, source || "");
    if (tool === "hand") {
      handLog("activate", { tool: tool, source: source || "" });
    }
    writeTool(tool);
    applyToolState(tool);

    if (tool === "hand") {
      // Same underlying mode as Space+Move: keep Penpot on the move tool.
      setScaleTextEnabled(false);
      if (moveBtnRef) {
        try {
          moveBtnRef.click();
        } catch (e) {
          error("moveBtn.click failed (hand)", e);
        }
      }
      return;
    }

    if (tool === "scale") {
      emitWorkspaceInterrupt();
      setScaleTextEnabled(true);
      if (moveBtnRef) {
        try {
          moveBtnRef.click();
        } catch (e) {
          error("moveBtn.click failed (scale)", e);
        }
      }
      return;
    }

    setScaleTextEnabled(false);
    if (moveBtnRef) {
      try {
        moveBtnRef.click();
      } catch (e) {
        error("moveBtn.click failed (move)", e);
      }
    }
  }

  function mountCombo(toolbar, moveBtn, moveLi) {
    if (!moveBtn || !moveLi) return;

    cleanupLegacyUi(toolbar.querySelector(OPTIONS_SEL));

    var combo = moveLi.querySelector('[data-logos-toolbar="move-combo"]');
    if (!combo) {
      moveLi.classList.add("logos-move-tool-slot");
      combo = document.createElement("div");
      combo.className = "logos-move-tool-combo";
      combo.setAttribute("data-logos-toolbar", "move-combo");
      combo.setAttribute("role", "group");
      combo.setAttribute("aria-label", "Move tools");

      var primary = document.createElement("button");
      primary.type = "button";
      primary.className = BTN_CLASS + " logos-move-tool-combo__main";
      primary.addEventListener("click", function (e) {
        if (e.target.closest(".logos-move-tool-combo__chevron")) return;
        e.stopPropagation();
        activateTool(readTool(), "combo-main");
      });

      var chevron = document.createElement("button");
      chevron.type = "button";
      chevron.className = "logos-move-tool-combo__chevron";
      chevron.setAttribute("aria-label", "More move tools");
      chevron.setAttribute("aria-haspopup", "menu");
      chevron.title = "More tools";
      chevron.setAttribute("data-logos-menu-bound", "1");
      chevron.innerHTML = systemIcon("chevronDown");
      chevron.addEventListener(
        "pointerdown",
        function (e) {
          if (e.button !== 0) return;
          e.preventDefault();
          e.stopPropagation();
          e.stopImmediatePropagation();
          if (openMenuEl && menuAnchorRef === combo) {
            closeMenu();
          } else {
            openMenu(combo, toolbar);
          }
        },
        true
      );

      combo.appendChild(primary);
      combo.appendChild(chevron);
      combo.addEventListener("contextmenu", function (e) {
        e.preventDefault();
        e.stopPropagation();
        openMenu(combo, toolbar);
      });
      moveLi.appendChild(combo);
      log("combo mounted on move slot");
    } else {
      var existingChevron = combo.querySelector(".logos-move-tool-combo__chevron");
      if (existingChevron && existingChevron.getAttribute("data-logos-menu-bound") !== "1") {
        var freshChevron = existingChevron.cloneNode(true);
        existingChevron.replaceWith(freshChevron);
        existingChevron = freshChevron;
        existingChevron.setAttribute("data-logos-menu-bound", "1");
        existingChevron.addEventListener(
          "pointerdown",
          function (e) {
            if (e.button !== 0) return;
            e.preventDefault();
            e.stopPropagation();
            e.stopImmediatePropagation();
            if (openMenuEl && menuAnchorRef === combo) {
              closeMenu();
            } else {
              openMenu(combo, toolbar);
            }
          },
          true
        );
      }
    }

    comboRef = combo;
    document.body.classList.add("logos-move-tools-ready");
    updateComboDisplay();
  }

  function isViewportTarget(target) {
    if (!target || !target.closest) return false;
    if (target.closest(CANVAS_IGNORE_SEL)) return false;
    if (openMenuEl && openMenuEl.contains(target)) return false;
    var app = document.getElementById("app");
    if (!app || !app.contains(target)) return false;
    if (target.closest(WORKSPACE_VIEWPORT_SEL)) return true;
    if (target.closest(VIEWPORT_SEL)) return true;
    if (target.closest(WORKSPACE_CONTENT_SEL)) {
      if (target.closest(".main_ui_workspace_sidebar")) return false;
      return true;
    }
    return false;
  }

  function isInputTarget(target) {
    if (!target) return false;
    return (
      target.tagName === "INPUT" ||
      target.tagName === "TEXTAREA" ||
      target.tagName === "SELECT" ||
      target.isContentEditable
    );
  }

  function emitStartPanning() {
    if (!panningApiReady()) {
      warnOnce("panning", "Workspace pan API not ready yet — wait for Penpot store to initialize.");
      return false;
    }
    viewportDragMode = "hand";
    handLog("panning started");
    log("panning started");
    return true;
  }

  function emitFinishViewportDrag() {
    if (!viewportDragMode) return;
    viewportDragMode = null;
    lastPanPoint = null;
    log("panning finished");
  }

  function pointCoord(APP, pt, axis) {
    if (!pt) return 0;
    if (typeof pt[axis] === "number") return pt[axis];
    var kw = axis === "x" ? APP.$cljs$cst$1500$x$$ : APP.$cljs$cst$1501$y$$;
    if (kw && kw.$cljs$core$IFn$_invoke$arity$1$) {
      return kw.$cljs$core$IFn$_invoke$arity$1$(pt);
    }
    return 0;
  }

  function emitPointerDelta(e) {
    var APP = penpotAPP();
    if (!APP || !APP.$app$common$geom$point$subtract$$) return;
    if (!APP.$app$main$data$workspace$viewport$update_viewport_position$$) return;
    var pt = cljsPoint(e.clientX, e.clientY);
    if (!pt) return;
    if (!lastPanPoint) {
      lastPanPoint = pt;
      return;
    }
    var delta = APP.$app$common$geom$point$subtract$$(pt, lastPanPoint);
    lastPanPoint = pt;
    var zoom = getViewportZoom();
    var dx = pointCoord(APP, delta, "x") / zoom;
    var dy = pointCoord(APP, delta, "y") / zoom;
    try {
      penpotEmit(
        APP.$app$main$data$workspace$viewport$update_viewport_position$$(
          new APP.$cljs$core$PersistentArrayMap$(
            null,
            2,
            [
              APP.$cljs$cst$1500$x$$,
              function (v) {
                return v - dx;
              },
              APP.$cljs$cst$1501$y$$,
              function (v) {
                return v - dy;
              },
            ],
            null
          )
        )
      );
    } catch (e2) {
      error("emitPointerDelta failed", e2);
    }
  }

  function clearPendingHandDrag() {
    pendingHandDrag = null;
  }

  function closeMenu() {
    if (menuDismissHandler) {
      document.removeEventListener("pointerdown", menuDismissHandler, true);
      menuDismissHandler = null;
    }
    menuAnchorRef = null;
    if (openMenuEl) {
      openMenuEl.remove();
      openMenuEl = null;
    }
  }

  function scheduleMenuDismiss() {
    if (menuDismissHandler) {
      document.removeEventListener("pointerdown", menuDismissHandler, true);
    }
    menuDismissHandler = function (e) {
      if (!openMenuEl) return;
      if (openMenuEl.contains(e.target)) return;
      if (menuAnchorRef && menuAnchorRef.contains(e.target)) return;
      closeMenu();
    };
    requestAnimationFrame(function () {
      requestAnimationFrame(function () {
        if (openMenuEl && menuDismissHandler) {
          document.addEventListener("pointerdown", menuDismissHandler, true);
        }
      });
    });
  }

  function toolbarPosition(toolbar) {
    return (toolbar && toolbar.dataset.logosToolbarPosition) || "bottom";
  }

  function placeMenu(menu, anchor, position) {
    var rect = anchor.getBoundingClientRect();
    var margin = 8;
    menu.style.position = "fixed";
    menu.style.zIndex = "10002";

    requestAnimationFrame(function () {
      var menuRect = menu.getBoundingClientRect();
      var top;
      var left;
      if (position === "bottom") {
        top = rect.top - menuRect.height - margin;
        left = rect.left + rect.width / 2 - menuRect.width / 2;
      } else if (position === "top") {
        top = rect.bottom + margin;
        left = rect.left + rect.width / 2 - menuRect.width / 2;
      } else if (position === "left") {
        top = rect.top + rect.height / 2 - menuRect.height / 2;
        left = rect.right + margin;
      } else {
        top = rect.top + rect.height / 2 - menuRect.height / 2;
        left = rect.left - menuRect.width - margin;
      }
      left = Math.max(margin, Math.min(left, window.innerWidth - menuRect.width - margin));
      top = Math.max(margin, Math.min(top, window.innerHeight - menuRect.height - margin));
      menu.style.top = top + "px";
      menu.style.left = left + "px";
    });
  }

  function openMenu(anchor, toolbar) {
    closeMenu();
    var current = readTool();
    var position = toolbarPosition(toolbar);

    var menu = document.createElement("div");
    menu.className = "logos-move-tool-menu";
    menu.setAttribute("role", "menu");
    menu.setAttribute("aria-label", "Move tools");

    TOOLS.forEach(function (tool) {
      var row = document.createElement("button");
      row.type = "button";
      row.className =
        "logos-move-tool-menu__row" +
        (tool === current ? " logos-move-tool-menu__row--active" : "");
      row.setAttribute("role", "menuitem");
      row.innerHTML =
        '<span class="logos-move-tool-menu__check" aria-hidden="true">' +
        (tool === current ? "✓" : "") +
        '</span><span class="logos-move-tool-menu__icon">' +
        toolIcon(tool) +
        '</span><span class="logos-move-tool-menu__label">' +
        LABELS[tool] +
        '</span><span class="logos-move-tool-menu__shortcut">' +
        SHORTCUTS[tool] +
        "</span>";
      row.addEventListener("click", function (e) {
        e.stopPropagation();
        activateTool(tool, "menu");
        closeMenu();
      });
      menu.appendChild(row);
    });

    document.body.appendChild(menu);
    openMenuEl = menu;
    menuAnchorRef = anchor;
    placeMenu(menu, anchor, position);
    log("menu opened", current);
    scheduleMenuDismiss();
  }

  function bindViewportDrag() {
    if (viewportDragBound) return;
    viewportDragBound = true;

    document.addEventListener(
      "pointerdown",
      function (e) {
        if (!isHandModeActive()) return;
        if (e.button !== 0) return;
        if (!isViewportTarget(e.target)) {
          if (readTool() === "hand") {
            handLog("pointerdown ignored (outside canvas)", describeTarget(e.target));
          }
          return;
        }
        if (!panningApiReady()) {
          handLog("pointerdown blocked — pan API not ready (reload if this persists)");
          return;
        }

        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation();

        handPointerSession = true;
        handLog(
          "pointerdown",
          describeTarget(e.target),
          "client",
          e.clientX,
          e.clientY,
          "tool",
          readTool(),
          "space",
          spacePanActive
        );

        var captureEl =
          document.getElementById("viewport-controls") ||
          e.target.closest("#viewport-controls") ||
          e.target.closest(VIEWPORT_SEL);
        if (captureEl && captureEl.setPointerCapture) {
          try {
            captureEl.setPointerCapture(e.pointerId);
          } catch (err) {
            /* ignore */
          }
        }

        lastPanPoint = cljsPoint(e.clientX, e.clientY);
        if (readTool() === "hand") {
          emitStartPanning();
        } else {
          pendingHandDrag = {
            x: e.clientX,
            y: e.clientY,
            pointerId: e.pointerId,
            captureEl: captureEl,
          };
        }
      },
      true
    );

    document.addEventListener(
      "pointermove",
      function (e) {
        if (pendingHandDrag && !viewportDragMode) {
          var dx = e.clientX - pendingHandDrag.x;
          var dy = e.clientY - pendingHandDrag.y;
          if (dx * dx + dy * dy >= DRAG_THRESHOLD_PX * DRAG_THRESHOLD_PX) {
            lastPanPoint = cljsPoint(pendingHandDrag.x, pendingHandDrag.y);
            if (emitStartPanning()) {
              emitPointerDelta(e);
            } else {
              lastPanPoint = null;
            }
            clearPendingHandDrag();
          }
        }
        if (viewportDragMode) {
          e.preventDefault();
          e.stopPropagation();
          emitPointerDelta(e);
        }
      },
      true
    );

    document.addEventListener(
      "pointerup",
      function (e) {
        if (handPointerSession) {
          e.preventDefault();
          e.stopPropagation();
          handLog("pointerup", "panning", !!viewportDragMode);
        }
        handPointerSession = false;
        clearPendingHandDrag();
        emitFinishViewportDrag();
      },
      true
    );

    document.addEventListener(
      "pointercancel",
      function () {
        handPointerSession = false;
        clearPendingHandDrag();
        emitFinishViewportDrag();
      },
      true
    );

    document.addEventListener(
      "click",
      function (e) {
        if (!isHandModeActive()) return;
        if (!isViewportTarget(e.target)) return;
        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation();
        handLog("click suppressed in pan mode", describeTarget(e.target));
      },
      true
    );

    log("viewport drag bound (hand pans immediately; space uses drag threshold)");
  }

  function bindKeyboardShortcuts() {
    if (shortcutsBound) return;
    shortcutsBound = true;

    document.addEventListener(
      "keydown",
      function (e) {
        if (isInputTarget(e.target)) return;

        if (e.code === "Space" || e.key === " ") {
          if (e.repeat) return;
          e.preventDefault();
          if (!spacePanActive) {
            spacePanActive = true;
            document.body.classList.add("logos-move-tool-space");
            log("space pan active");
          }
          return;
        }

		if (e.repeat || e.metaKey || e.ctrlKey || e.altKey) return;

		var key = e.key.toLowerCase();
		if (key === "v") {
			e.stopPropagation();
			activateTool("move", "keyboard");
		} else if (key === "h") {
			e.preventDefault();
			e.stopPropagation();
			activateTool("hand", "keyboard");
		} else if (key === "k") {
			e.preventDefault();
			e.stopPropagation();
			activateTool("scale", "keyboard");
		}
      },
      true
    );

    document.addEventListener(
      "keyup",
      function (e) {
        if (e.code !== "Space" && e.key !== " ") return;
        if (!spacePanActive) return;
        spacePanActive = false;
        document.body.classList.remove("logos-move-tool-space");
        // Hand tool stays in pan mode; only end drag when Space was the pan source.
        if (readTool() !== "hand") {
          clearPendingHandDrag();
          emitFinishViewportDrag();
        }
        log("space pan released");
      },
      true
    );
  }

  function watchPenpotSelection() {
    if (!moveBtnRef) return;
    var observer = new MutationObserver(function () {
      updateComboDisplay();
    });
    observer.observe(moveBtnRef, { attributes: true, attributeFilter: ["class"] });
  }

  function enhanceToolbar(toolbar) {
    if (!toolbar) return;
    toolbarRef = toolbar;
    var options = toolbar.querySelector(OPTIONS_SEL);
    if (!options) return;

    moveBtnRef = findMoveButton(toolbar);
    var moveLi = options.querySelector("li:first-child");
    if (!moveBtnRef || !moveLi) return;

    mountCombo(toolbar, moveBtnRef, moveLi);
    if (watchedMoveSlots && !watchedMoveSlots.has(moveLi)) {
      watchedMoveSlots.add(moveLi);
      watchPenpotSelection();
    }
    if (!initialToolApplied) {
      initialToolApplied = true;
      activateTool(DEFAULT_TOOL, "toolbar-mount");
    } else {
      applyToolState(readTool());
    }
  }

  function scan() {
    try {
      document.querySelectorAll(TOOLBAR_SEL).forEach(enhanceToolbar);
    } catch (e) {
      error("scan failed", e);
    }
  }

  var scanScheduled = false;
  function scheduleScan() {
    if (scanScheduled) return;
    scanScheduled = true;
    requestAnimationFrame(function () {
      scanScheduled = false;
      scan();
    });
  }

  function observeToolbarOnce() {
    var app = document.getElementById("app");
    if (!app) return;
    var observer = new MutationObserver(function () {
      if (!comboRef) {
        scheduleScan();
      }
    });
    observer.observe(app, { childList: true, subtree: true });
    setTimeout(function () {
      observer.disconnect();
    }, 15000);
  }

  function init() {
    if (initDone) return;
    initDone = true;
    try {
      bindViewportDrag();
      bindKeyboardShortcuts();
      scan();
      observeToolbarOnce();
      globalThis.logosMoveToolStatus = function () {
        return {
          build: BUILD,
          tool: readTool(),
          panModeActive: isPanModeActive(),
          panningApiReady: panningApiReady(),
          penpotApp: !!penpotAPP(),
          viewportZoom: getViewportZoom(),
          scaleTextEnabled: isScaleTextEnabled(),
          viewportDragMode: viewportDragMode,
          spacePanActive: spacePanActive,
          moveBtn: !!moveBtnRef,
          combo: !!comboRef,
          debug: DEBUG,
        };
      };
      log("init complete");
    } catch (e) {
      error("init failed", e);
    }
  }

  function startWhenWorkspaceReady() {
    var attempts = 0;
    function tick() {
      attempts++;
      var loader = document.querySelector(".main_ui_workspace__workspace-loader");
      var content = document.querySelector(".main_ui_workspace__workspace-content");
      if ((!loader && content) || attempts > 900) {
        init();
        return;
      }
      requestAnimationFrame(tick);
    }
    tick();
  }

  console.info(
    LOG_PREFIX,
    "build=" + BUILD,
    "(Figma Move/Hand/Scale; hand mirrors Space+Move pan)"
  );

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", startWhenWorkspaceReady);
  } else {
    startWhenWorkspaceReady();
  }
})();
