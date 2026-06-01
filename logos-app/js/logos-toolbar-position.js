/**
 * Logos — repositionable Penpot workspace toolbar (bottom default, Figma-style).
 * Icons: LogosSystemIcons (Font Awesome sharp-solid via logos-workspace-icons.js).
 */
(function () {
  "use strict";

  if (
    localStorage.getItem("logos:workspace:no-toolbar") === "1" ||
    /(?:^|[?&])logos-no-toolbar(?:=1)?(?:&|$)/.test(location.search)
  ) {
    return;
  }

  const STORAGE_KEY = "logos:workspace:toolbar-position";
  const POSITIONS = ["bottom", "top", "left", "right"];
  const DEFAULT_POSITION = "bottom";

  const TOOLBAR_SEL = ".main_ui_workspace_top_toolbar__main-toolbar";
  const OPTIONS_SEL = ".main_ui_workspace_top_toolbar__main-toolbar-options";
  const BTN_CLASS = "main_ui_workspace_top_toolbar__main-toolbar-options-button";

  const LABELS = {
    bottom: "Bottom",
    top: "Top",
    left: "Left",
    right: "Right",
  };

  const MENU_ICON_KEYS = {
    bottom: "positionBottom",
    top: "positionTop",
    left: "positionLeft",
    right: "positionRight",
  };

  let openMenuEl = null;

  function readPosition() {
    const stored = localStorage.getItem(STORAGE_KEY);
    return POSITIONS.includes(stored) ? stored : DEFAULT_POSITION;
  }

  function writePosition(position) {
    localStorage.setItem(STORAGE_KEY, position);
  }

  function systemIcon(key) {
    const icons = globalThis.LogosSystemIcons;
    if (icons && icons[key]) {
      return icons[key];
    }
    return fallbackIcon(key);
  }

  /** Minimal fallback if logos-workspace-icons.js failed to load. */
  function fallbackIcon(key) {
    const label = key.replace(/^position/i, "") || "position";
    return (
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="500" height="500" ' +
      'class="logos-system-icon logos-system-icon--' +
      key +
      '" fill="currentColor" aria-hidden="true">' +
      '<rect x="2" y="2" width="12" height="12" rx="1.5" fill="none" stroke="currentColor" stroke-width="1"/>' +
      '<text x="8" y="11" text-anchor="middle" font-size="6" fill="currentColor" stroke="none">' +
      label.charAt(0).toUpperCase() +
      "</text></svg>"
    );
  }

  function toolbarPositionButtonIcon() {
    return systemIcon("position");
  }

  function menuPositionIcon(position) {
    return systemIcon(MENU_ICON_KEYS[position] || "position");
  }

  function applyPosition(toolbar, position) {
    POSITIONS.forEach(function (p) {
      toolbar.classList.toggle("logos-toolbar-position-" + p, p === position);
    });
    toolbar.classList.toggle(
      "logos-toolbar-vertical",
      position === "left" || position === "right"
    );
    toolbar.dataset.logosToolbarPosition = position;
    keepToolbarExpanded(toolbar);
  }

  /** Penpot collapse strip is disabled — always show full toolbar. */
  function keepToolbarExpanded(toolbar) {
    if (!toolbar) return;
    toolbar.classList.remove("main_ui_workspace_top_toolbar__main-toolbar-hidden");
    var handler = toolbar.querySelector(".main_ui_workspace_top_toolbar__toolbar-handler");
    if (handler) {
      handler.hidden = true;
    }
  }

  function closeMenu() {
    if (openMenuEl) {
      openMenuEl.remove();
      openMenuEl = null;
    }
  }

  function placeMenu(menu, anchor, toolbarPosition) {
    const rect = anchor.getBoundingClientRect();
    const margin = 8;
    menu.style.position = "fixed";
    menu.style.zIndex = "10000";

    requestAnimationFrame(function () {
      const menuRect = menu.getBoundingClientRect();
      let top;
      let left;

      if (toolbarPosition === "bottom") {
        top = rect.top - menuRect.height - margin;
        left = rect.left + rect.width / 2 - menuRect.width / 2;
      } else if (toolbarPosition === "top") {
        top = rect.bottom + margin;
        left = rect.left + rect.width / 2 - menuRect.width / 2;
      } else if (toolbarPosition === "left") {
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
    const current = readPosition();

    const menu = document.createElement("div");
    menu.className = "logos-toolbar-position-menu";
    menu.setAttribute("role", "menu");
    menu.setAttribute("aria-label", "Toolbar position");

    POSITIONS.filter(function (p) {
      return p !== current;
    }).forEach(function (p) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "logos-toolbar-position-menu__row";
      btn.setAttribute("role", "menuitem");
      btn.innerHTML =
        '<span class="logos-toolbar-position-menu__icon">' +
        menuPositionIcon(p) +
        '</span><span class="logos-toolbar-position-menu__label">' +
        LABELS[p] +
        "</span>";
      btn.addEventListener("click", function (e) {
        e.stopPropagation();
        writePosition(p);
        applyPosition(toolbar, p);
        closeMenu();
      });
      menu.appendChild(btn);
    });

    document.body.appendChild(menu);
    openMenuEl = menu;
    placeMenu(menu, anchor, current);

    setTimeout(function () {
      document.addEventListener(
        "click",
        function () {
          closeMenu();
        },
        { once: true, capture: true }
      );
    }, 0);
  }

  function injectPositionButton(toolbar) {
    const options = toolbar.querySelector(OPTIONS_SEL);
    if (!options) return;

    let item = options.querySelector('[data-logos-toolbar="position-item"]');
    if (!item) {
      item = document.createElement("li");
      item.className = "logos-toolbar-position-item";
      item.setAttribute("data-logos-toolbar", "position-item");

      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = BTN_CLASS + " logos-toolbar-position-btn";
      btn.setAttribute("data-logos-toolbar", "position-btn");
      btn.title = "Position";
      btn.setAttribute("aria-label", "Toolbar position");
      btn.setAttribute("aria-haspopup", "menu");
      btn.innerHTML = toolbarPositionButtonIcon();

      btn.addEventListener("click", function (e) {
        e.stopPropagation();
        if (openMenuEl) {
          closeMenu();
        } else {
          openMenu(btn, toolbar);
        }
      });

      item.appendChild(btn);
      options.appendChild(item);
    } else {
      const btn = item.querySelector('[data-logos-toolbar="position-btn"]');
      if (btn && !btn.querySelector(".logos-system-icon")) {
        btn.innerHTML = toolbarPositionButtonIcon();
      }
    }

    applyPosition(toolbar, readPosition());
  }

  function enhanceToolbar(toolbar) {
    if (!toolbar) return;
    keepToolbarExpanded(toolbar);
    if (toolbar.dataset.logosToolbarBound === "1") return;
    injectPositionButton(toolbar);
    toolbar.dataset.logosToolbarBound = "1";
    var collapseGuard = new MutationObserver(function () {
      if (toolbar.classList.contains("main_ui_workspace_top_toolbar__main-toolbar-hidden")) {
        keepToolbarExpanded(toolbar);
      }
    });
    collapseGuard.observe(toolbar, { attributes: true, attributeFilter: ["class"] });
    stopObserving();
  }

  function scan() {
    document.querySelectorAll(TOOLBAR_SEL).forEach(enhanceToolbar);
  }

  let scanScheduled = false;
  function scheduleScan() {
    if (scanScheduled) return;
    scanScheduled = true;
    requestAnimationFrame(function () {
      scanScheduled = false;
      scan();
    });
  }

  const observer = new MutationObserver(function () {
    scheduleScan();
  });
  let observing = false;

  function stopObserving() {
    if (!observing) {
      return;
    }
    observer.disconnect();
    observing = false;
  }

  function observeTarget() {
    if (observing) {
      return;
    }
    var app = document.getElementById("app");
    if (app) {
      observer.observe(app, { childList: true, subtree: true });
    } else {
      observer.observe(document.body, { childList: true, subtree: true });
    }
    observing = true;
  }

  function init() {
    scan();
    observeTarget();
  }

  /** Wait until Penpot drops the workspace loader before touching the DOM tree. */
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

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", startWhenWorkspaceReady);
  } else {
    startWhenWorkspaceReady();
  }
})();
