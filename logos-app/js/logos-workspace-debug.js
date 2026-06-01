/**
 * Logos workspace deep diagnostics (browser / Penpot frontend).
 *
 * Enabled when localStorage logos:workspace:debug=1 or ?logos-debug=1
 * Disable explicitly: localStorage.setItem('logos:workspace:debug', '0')
 *
 * Console filter: [logos-ws]
 * Manual snapshot: logosWorkspaceDiag()
 * Trace ring buffer (survives tab close): logosWorkspaceTrace()
 * Disable toolbar scripts for isolation: ?logos-no-toolbar=1
 */
(function () {
  "use strict";

  var STORAGE_KEY = "logos:workspace:debug";
  var TRACE_KEY = "logos:ws:trace";
  var FREEZE_KEY = "logos:ws:last-freeze";
  var SESSION_KEY = "logos:ws:session";
  var TRACE_MAX = 300;

  /** Persist across tab close (sessionStorage is per-tab). */
  function loadTrace() {
    try {
      return JSON.parse(localStorage.getItem(TRACE_KEY) || "[]");
    } catch (e) {
      return [];
    }
  }

  function saveTrace(buf) {
    try {
      localStorage.setItem(TRACE_KEY, JSON.stringify(buf));
    } catch (e) {
      /* quota */
    }
  }

  var enabled =
    localStorage.getItem(STORAGE_KEY) === "1" ||
    /(?:^|[?&])logos-debug(?:=1)?(?:&|$)/.test(location.search);

  if (!enabled) return;

  var t0 = performance.now();
  var lastMilestone = "boot";
  var pendingFetches = Object.create(null);

  function ts() {
    return (performance.now() - t0).toFixed(0) + "ms";
  }

  function LOG() {
    var args = ["%c[logos-ws]%c +" + ts(), "color:#6ea8fe;font-weight:700", "color:inherit"];
    for (var i = 0; i < arguments.length; i++) args.push(arguments[i]);
    console.info.apply(console, args);
  }

  function WARN() {
    var args = ["%c[logos-ws]%c WARN +" + ts(), "color:#ffc107;font-weight:700", "color:inherit"];
    for (var i = 0; i < arguments.length; i++) args.push(arguments[i]);
    console.warn.apply(console, args);
  }

  function persistTrace(event, detail) {
    var entry = {
      at: new Date().toISOString(),
      ms: Math.round(performance.now() - t0),
      event: event,
      milestone: lastMilestone,
      href: location.href,
      detail: detail || null,
    };
    try {
      var buf = loadTrace();
      buf.push(entry);
      if (buf.length > TRACE_MAX) buf = buf.slice(-TRACE_MAX);
      saveTrace(buf);
    } catch (e) {
      /* quota / private mode */
    }
    return entry;
  }

  function setMilestone(name, detail) {
    lastMilestone = name;
    LOG("milestone:" + name, detail || "");
    persistTrace("milestone:" + name, detail || null);
  }

  function parseWorkspaceHash() {
    var m = (location.hash || "").match(/^#\/workspace(?:\/([^/?#]+)\/([^/?#]+))?(?:\?([^#]*))?/);
    if (!m) return null;
    var params = new URLSearchParams(m[3] || "");
    return {
      teamId: params.get("team-id") || params.get("teamId") || "",
      fileId: params.get("file-id") || params.get("fileId") || m[2] || "",
      pageId: params.get("page-id") || params.get("pageId") || "",
      legacyProjectId: m[1] || "",
      raw: location.hash,
    };
  }

  function loaderVisible() {
    return !!document.querySelector(
      ".main_ui_workspace__workspace-loader," +
        '[class*="workspace-loader"],' +
        '[class*="loader"][class*="overlay"]'
    );
  }

  function readStoreKey(key) {
    if (typeof debug === "undefined" || !debug.get_state) return undefined;
    var captured;
    var prev = console.log;
    console.log = function (v) {
      captured = v;
    };
    try {
      debug.get_state(key);
    } catch (e) {
      captured = { error: String(e) };
    }
    console.log = prev;
    return captured;
  }

  function penpotStateSnapshot() {
    var snap = {
      href: location.href,
      pathname: location.pathname,
      hash: location.hash,
      workspace: parseWorkspaceHash(),
      loaderVisible: loaderVisible(),
      hasApp: !!document.getElementById("app"),
      hasWorkspaceContent: !!document.querySelector(".main_ui_workspace__workspace-content"),
      hasViewport: !!document.querySelector(".main_ui_workspace_viewport__viewport"),
      penpotStartDate: globalThis.penpotStartDate || null,
      onbeforeunload: typeof window.onbeforeunload === "function",
      pendingFetches: Object.keys(pendingFetches),
      lastMilestone: lastMilestone,
      penpotPathOk:
        location.pathname === "/" ||
        location.pathname === "" ||
        location.pathname.endsWith("/"),
    };

    if (typeof features !== "undefined" && features.get_enabled) {
      try {
        snap.features = features.get_enabled();
      } catch (e) {
        snap.featuresError = String(e);
      }
    }

    if (typeof debug !== "undefined" && debug.get_state) {
      var keys = [
        "current-file-id",
        "current-page-id",
        "workspace-ready",
        "route",
      ];
      snap.store = {};
      keys.forEach(function (key) {
        snap.store[key] = readStoreKey(key);
      });
    }

    return snap;
  }

  function logSnapshot(label, extra) {
    var snap = penpotStateSnapshot();
    if (extra) Object.assign(snap, extra);
    LOG("snapshot:" + label, snap);
    if (snap.penpotPathOk === false) {
      WARN("pathname is not app root — Penpot router will 404", {
        pathname: snap.pathname,
        hint: "URL should be /#/workspace?… not /workspace.html#/…",
      });
    }
    if (snap.loaderVisible && snap.store && snap.store["workspace-ready"]) {
      WARN("loader still visible but workspace-ready is set — UI/router mismatch?", snap);
    }
    if (snap.loaderVisible && snap.store && snap.store["current-page-id"] && !snap.store["workspace-ready"]) {
      WARN("loader visible, page-id set, workspace not ready yet", snap);
    }
    if (snap.loaderVisible && snap.workspace && snap.workspace.pageId) {
      var curPage = snap.store && snap.store["current-page-id"];
      var curFile = snap.store && snap.store["current-file-id"];
      if (curFile && !curPage) {
        WARN(
          "STALL: file is in store but current-page-id is unset — page init did not run",
          { urlPageId: snap.workspace.pageId, currentFileId: curFile }
        );
      } else if (curPage && snap.workspace.pageId && String(curPage) !== String(snap.workspace.pageId)) {
        WARN("STALL: URL page-id does not match store current-page-id", {
          urlPageId: snap.workspace.pageId,
          currentPageId: curPage,
        });
      } else if (curPage && !snap.hasWorkspaceContent && !snap.hasViewport) {
        WARN("STALL: current-page-id set but viewport/content missing — render or wasm hang?", {
          currentPageId: curPage,
          workspaceReady: snap.store["workspace-ready"],
        });
      }
    }
    return snap;
  }

  function diagnoseStall(reason) {
    var snap = logSnapshot("stall:" + reason);
    WARN("stall diagnosis", {
      reason: reason,
      hasWorkspaceContent: snap.hasWorkspaceContent,
      hasViewport: snap.hasViewport,
      wasmScript: !!document.querySelector('script[src*="render-wasm"]'),
      store: snap.store,
      pendingFetches: snap.pendingFetches,
      lastMilestone: lastMilestone,
    });
    persistTrace("stall:" + reason, {
      store: snap.store,
      pendingFetches: snap.pendingFetches,
    });
    return snap;
  }

  // ── Main-thread freeze detector (works when setTimeout stops firing) ────────
  var heartbeatLast = performance.now();
  var freezeReported = false;

  function reportFreeze(gapMs, source) {
    if (freezeReported) return;
    freezeReported = true;
    var payload = {
      gapMs: Math.round(gapMs),
      source: source,
      lastMilestone: lastMilestone,
      pendingFetches: Object.keys(pendingFetches),
      loaderVisible: loaderVisible(),
      href: location.href,
    };
    try {
      localStorage.setItem(FREEZE_KEY, JSON.stringify(payload));
    } catch (e) {
      /* ignore */
    }
    persistTrace("MAIN_THREAD_FREEZE", payload);
    WARN("MAIN_THREAD_FREEZE — event loop blocked", payload);
    try {
      logSnapshot("freeze");
    } catch (e) {
      /* may not run if still blocked */
    }
  }

  var heartbeatTimer = null;
  var freezeTimer = null;

  function startHeartbeatMonitors() {
    if (heartbeatTimer || freezeTimer || document.hidden) {
      return;
    }
    try {
      var ch = new MessageChannel();
      ch.port1.onmessage = function () {
        heartbeatLast = performance.now();
        if (!document.hidden) {
          ch.port2.postMessage(null);
        }
      };
      ch.port2.postMessage(null);
      heartbeatTimer = setInterval(function () {
        if (!document.hidden) {
          ch.port2.postMessage(null);
        }
      }, 200);
    } catch (e) {
      /* MessageChannel unavailable */
    }
    freezeTimer = setInterval(function () {
      if (document.hidden) {
        return;
      }
      var gap = performance.now() - heartbeatLast;
      if (gap > 800) reportFreeze(gap, "message-channel");
    }, 500);
  }

  function stopHeartbeatMonitors() {
    if (heartbeatTimer) {
      clearInterval(heartbeatTimer);
      heartbeatTimer = null;
    }
    if (freezeTimer) {
      clearInterval(freezeTimer);
      freezeTimer = null;
    }
  }

  document.addEventListener("visibilitychange", function () {
    if (document.hidden) {
      stopHeartbeatMonitors();
    } else {
      heartbeatLast = performance.now();
      startHeartbeatMonitors();
    }
  });

  startHeartbeatMonitors();

  if (window.PerformanceObserver) {
    try {
      var ltObs = new PerformanceObserver(function (list) {
        list.getEntries().forEach(function (entry) {
          if (entry.duration >= 300) {
            persistTrace("longtask", {
              duration: Math.round(entry.duration),
              start: Math.round(entry.startTime),
              name: entry.name,
            });
            WARN("longtask", {
              durationMs: Math.round(entry.duration),
              lastMilestone: lastMilestone,
            });
          }
        });
      });
      ltObs.observe({ type: "longtask", buffered: true });
    } catch (e) {
      /* longtask unsupported */
    }
  }

  // ── Boot ───────────────────────────────────────────────────────────────────
  var sessionId = Date.now().toString(36) + "-" + Math.random().toString(36).slice(2, 8);
  try {
    sessionStorage.setItem(SESSION_KEY, sessionId);
  } catch (e) {
    /* ignore */
  }

  try {
    var prevFreeze = localStorage.getItem(FREEZE_KEY);
    if (prevFreeze) {
      WARN("previous load reported MAIN_THREAD_FREEZE", JSON.parse(prevFreeze));
      localStorage.removeItem(FREEZE_KEY);
    }
    var prevTrace = loadTrace();
    if (prevTrace.length) {
      var tail = prevTrace.slice(-8);
      LOG("previous trace tail (localStorage)", tail);
    }
  } catch (e) {
    /* ignore */
  }

  setMilestone("debug-enabled", {
    href: location.href,
    pathname: location.pathname,
    hash: location.hash,
    workspace: parseWorkspaceHash(),
    sessionId: sessionId,
    userAgent: navigator.userAgent.slice(0, 80),
  });

  // ── History ────────────────────────────────────────────────────────────────
  ["pushState", "replaceState"].forEach(function (method) {
    var orig = history[method];
    if (!orig) return;
    history[method] = function (state, title, url) {
      LOG("history." + method, { from: location.href, to: url, state: state });
      if (
        url &&
        typeof url === "string" &&
        url.indexOf("workspace.html") >= 0 &&
        url.indexOf("#/workspace") >= 0
      ) {
        WARN(
          "history." + method + " kept /workspace.html — Penpot requires pathname / (public_uri root)",
          { attempted: url, expected: url.replace(/\/workspace\.html(?=#)/, "") }
        );
      }
      return orig.apply(this, arguments);
    };
  });

  window.addEventListener(
    "popstate",
    function (e) {
      LOG("popstate", { state: e.state, href: location.href, hash: location.hash });
      logSnapshot("after-popstate");
    },
    true
  );

  window.addEventListener(
    "hashchange",
    function (e) {
      LOG("hashchange", { oldURL: e.oldURL, newURL: e.newURL });
      logSnapshot("after-hashchange");
    },
    true
  );

  window.addEventListener(
    "beforeunload",
    function (e) {
      persistTrace("beforeunload", {
        returnValue: e.returnValue,
        onbeforeunload: typeof window.onbeforeunload,
      });
      LOG("beforeunload fired", {
        href: location.href,
        returnValue: e.returnValue,
        onbeforeunload: typeof window.onbeforeunload,
      });
    },
    true
  );

  var _onbeforeunload = window.onbeforeunload;
  try {
    Object.defineProperty(window, "onbeforeunload", {
      configurable: true,
      enumerable: true,
      get: function () {
        return _onbeforeunload;
      },
      set: function (fn) {
        LOG("window.onbeforeunload assigned", { fn: typeof fn });
        persistTrace("onbeforeunload-set", { fn: typeof fn });
        _onbeforeunload = fn;
      },
    });
  } catch (e) {
    /* ignore */
  }

  // Penpot workspace log hints (app.main.data.workspace)
  var penpotHints = {
    "initialize-workspace": "initialize_workspace",
    "initialize-file": "initialize_file",
    "fetch bundle": "fetch_bundle",
    "resolve file": "resolve_file",
    "file resolved": "file_resolved",
    "bundle fetched": "bundle_fetched",
  };

  function tryHookPenpotLogging() {
    if (typeof shared === "undefined" || !shared.app || !shared.app.common) return false;
    return false; // compiled bundle doesn't expose logging hook reliably
  }

  // ── Worker / script load (Worker() bypasses fetch hook) ─────────────────────
  var workerScriptUrl = null;
  var workerCreatedAt = null;

  if (window.Worker) {
    var OrigWorker = window.Worker;
    window.Worker = function (scriptURL, options) {
      workerScriptUrl = String(scriptURL);
      workerCreatedAt = performance.now();
      LOG("Worker ctor", { url: workerScriptUrl });
      persistTrace("worker-ctor", { url: workerScriptUrl });
      var instance = new OrigWorker(scriptURL, options);
      instance.addEventListener("error", function (ev) {
        WARN("Worker script error", { url: workerScriptUrl, message: ev.message, lineno: ev.lineno });
        persistTrace("worker-error", { url: workerScriptUrl, message: ev.message });
      });
      return instance;
    };
    window.Worker.prototype = OrigWorker.prototype;
  }

  function pendingScriptResources() {
    try {
      return performance
        .getEntriesByType("resource")
        .filter(function (e) {
          return (
            (/\.js(\?|$)/.test(e.name) || /worker\/main\.js/.test(e.name)) &&
            e.responseEnd === 0
          );
        })
        .map(function (e) {
          return {
            name: e.name.split("?")[0].replace(location.origin, ""),
            waitingMs: Math.round(performance.now() - e.startTime),
            transferSize: e.transferSize,
          };
        });
    } catch (e) {
      return [];
    }
  }

  function warnConnectionStarvation() {
    var pending = pendingScriptResources();
    var apiPending = Object.keys(pendingFetches);
    if (!pending.length && !apiPending.length) return;
    var workerPending = pending.filter(function (p) {
      return p.name.indexOf("worker/main.js") >= 0;
    });
    if (workerPending.length || (workerScriptUrl && workerCreatedAt && performance.now() - workerCreatedAt > 3000)) {
      WARN("possible HTTP/1.1 connection starvation — worker or API stalled", {
        workerScriptUrl: workerScriptUrl,
        workerPending: workerPending,
        pendingScripts: pending.slice(0, 8),
        pendingApi: apiPending,
        hint: "Hard refresh re-downloads ~30MB JS; reload without cache bypass or use DevTools Disable cache off",
      });
      persistTrace("connection-starvation", {
        workerPending: workerPending,
        pendingScripts: pending,
        pendingApi: apiPending,
      });
    }
  }

  // ── Network ─────────────────────────────────────────────────────────────────
  function trackFetchStart(url) {
    pendingFetches[url] = performance.now();
    if (Object.keys(pendingFetches).length === 1) {
      persistTrace("fetch-pending-first", { url: url });
    }
  }

  function trackFetchEnd(url, meta) {
    delete pendingFetches[url];
    var hint = null;
    if (/get-file(?:[?/]|$)/.test(url)) hint = "get-file";
    else if (/get-project/.test(url)) hint = "get-project";
    else if (/get-file-libraries/.test(url)) hint = "get-file-libraries";
    else if (/get-file-object-thumbnails/.test(url)) hint = "get-thumbnails";
    else if (/render-wasm/.test(url)) hint = "render-wasm";
    if (hint) {
      setMilestone("fetch:" + hint + ":done", meta);
    }
    if (/get-file(?:[?/]|$)/.test(url) && meta && meta.status === 200) {
      setMilestone("after-get-file-response", meta);
      [500, 1500, 3000, 5000, 10000, 20000].forEach(function (delay) {
        setTimeout(function () {
          if (loaderVisible()) {
            warnConnectionStarvation();
            diagnoseStall("after-get-file+" + delay + "ms");
          }
        }, delay);
      });
    }
  }

  var origFetch = window.fetch;
  if (origFetch) {
    window.fetch = function (input, init) {
      var url =
        typeof input === "string"
          ? input
          : input && typeof input.url === "string"
            ? input.url
            : String(input);
      var isApi = url.indexOf("/api/") >= 0;
      var t = performance.now();
      if (isApi) {
        LOG("fetch →", url, init && init.method);
        trackFetchStart(url);
      }
      return origFetch.apply(this, arguments).then(function (res) {
        if (isApi) {
          var ms = (performance.now() - t).toFixed(1);
          var meta = { status: res.status, ms: ms };
          // Avoid cloning multi-MB get-file payloads — blocks the main thread.
          if (/get-file(?:[?/]|$)/.test(url)) {
            meta.note = "body size skipped (get-file)";
            LOG("fetch ←", url, meta);
            if (res.status >= 400) WARN("fetch error response", url, res.status);
            trackFetchEnd(url, meta);
          } else if (/get-file-object-thumbnails|get-project|get-file-libraries|get-profile|get-teams/.test(url)) {
            LOG("fetch ←", url, meta);
            if (res.status >= 400) WARN("fetch error response", url, res.status);
            trackFetchEnd(url, meta);
          } else {
            var clone = res.clone();
            clone
              .arrayBuffer()
              .then(function (buf) {
                meta.bytes = buf.byteLength;
                LOG("fetch ←", url, meta);
                if (res.status >= 400) WARN("fetch error response", url, res.status);
                if (res.ok && buf.byteLength === 0) WARN("fetch empty body on success", url);
                trackFetchEnd(url, meta);
              })
              .catch(function () {
                LOG("fetch ←", url, meta);
                trackFetchEnd(url, meta);
              });
          }
        }
        return res;
      });
    };
  }

  var OrigXHR = window.XMLHttpRequest;
  if (OrigXHR) {
    window.XMLHttpRequest = function () {
      var xhr = new OrigXHR();
      var reqUrl = "";
      var open = xhr.open;
      xhr.open = function (method, url) {
        reqUrl = url;
        if (String(url).indexOf("/api/") >= 0) {
          LOG("xhr →", method, url);
          trackFetchStart(String(url));
        }
        return open.apply(this, arguments);
      };
      xhr.addEventListener("load", function () {
        if (reqUrl.indexOf("/api/") >= 0) {
          LOG("xhr ←", reqUrl, { status: xhr.status, bytes: (xhr.responseText || "").length });
          trackFetchEnd(reqUrl, { status: xhr.status, bytes: (xhr.responseText || "").length });
        }
      });
      return xhr;
    };
  }

  // ── Errors ──────────────────────────────────────────────────────────────────
  window.addEventListener("error", function (e) {
    persistTrace("window.error", {
      message: e.message,
      file: e.filename,
      line: e.lineno,
    });
    WARN("window.error", e.message, e.filename, e.lineno, e.colno, e.error);
  });
  window.addEventListener("unhandledrejection", function (e) {
    persistTrace("unhandledrejection", { reason: String(e.reason) });
    WARN("unhandledrejection", e.reason);
  });

  // ── Penpot milestones (poll until workspace ready or timeout) ───────────────
  var milestones = {
    penpotInit: false,
    loaderSeen: false,
    loaderGone: false,
    workspaceReady: false,
    pageIdSet: false,
  };

  function checkMilestones() {
    if (globalThis.penpotStartDate && !milestones.penpotInit) {
      milestones.penpotInit = true;
      setMilestone("penpot-init-started", { penpotStartDate: globalThis.penpotStartDate });
    }

    var loading = loaderVisible();
    if (loading && !milestones.loaderSeen) {
      milestones.loaderSeen = true;
      setMilestone("loader-visible");
    }
    if (!loading && milestones.loaderSeen && !milestones.loaderGone) {
      milestones.loaderGone = true;
      setMilestone("loader-hidden");
      logSnapshot("loader-hidden");
    }

    if (typeof debug !== "undefined" && debug.get_state) {
      try {
        var ready = readStoreKey("workspace-ready");
        if (ready && !milestones.workspaceReady) {
          milestones.workspaceReady = true;
          setMilestone("workspace-ready", ready);
          logSnapshot("workspace-ready");
        }
        var pageId = readStoreKey("current-page-id");
        if (pageId && !milestones.pageIdSet) {
          milestones.pageIdSet = true;
          setMilestone("current-page-id-set", pageId);
        }
      } catch (e) {
        /* penpot store not ready */
      }
    }
  }

  var pollCount = 0;
  var pollMax = 120;
  var pollTimer = setInterval(function () {
    pollCount++;
    checkMilestones();
    if (pollCount % 5 === 0 && !milestones.workspaceReady) {
      warnConnectionStarvation();
    }
    if (pollCount % 10 === 0 && !milestones.workspaceReady) {
      logSnapshot("poll-" + pollCount);
    }
    if (milestones.workspaceReady && milestones.loaderGone) {
      setMilestone("boot-complete");
      clearInterval(pollTimer);
    }
    if (pollCount >= pollMax) {
      WARN("milestone: timeout — still loading after " + pollMax + "s", milestones);
      diagnoseStall("poll-timeout-" + pollMax + "s");
      clearInterval(pollTimer);
    }
  }, 1000);

  document.addEventListener("DOMContentLoaded", function () {
    setMilestone("dom-content-loaded");
    logSnapshot("dom-ready");
  });

  window.addEventListener("load", function () {
    setMilestone("window-load");
    logSnapshot("window-load");
    tryHookPenpotLogging();
  });

  document.addEventListener("penpot:wasm:loaded", function () {
    setMilestone("penpot-wasm-loaded");
  });
  document.addEventListener("penpot:wasm:render", function () {
    setMilestone("penpot-wasm-render");
  });

  // ── Public API ────────────────────────────────────────────────────────────
  globalThis.logosWorkspaceDiag = function () {
    return logSnapshot("manual");
  };

  globalThis.logosWorkspaceReadStore = function (key) {
    return readStoreKey(key || "current-page-id");
  };

  globalThis.logosWorkspaceTrace = function (clear) {
    try {
      var buf = loadTrace();
      if (clear) localStorage.removeItem(TRACE_KEY);
      return buf;
    } catch (e) {
      return [];
    }
  };

  globalThis.logosWorkspaceDebugOff = function () {
    localStorage.setItem(STORAGE_KEY, "0");
    LOG("debug disabled — reload page");
  };

  globalThis.logosWorkspaceNoToolbar = function () {
    localStorage.setItem("logos:workspace:no-toolbar", "1");
    LOG("toolbar scripts disabled on next load — reload page");
  };

  LOG("helpers: logosWorkspaceDiag(), logosWorkspaceTrace(), logosWorkspaceDebugOff(), logosWorkspaceNoToolbar()");
})();
