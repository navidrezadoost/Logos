import { useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { getProfile, workspaceURL } from "./api/client";

/**
 * Legacy URLs used hash routing, e.g.
 *   #/dashboard/recent?team-id=…
 *   #/workspace/:projectId/:fileId?page-id=…
 * Redirect them to the React router paths on first load.
 */
export function LegacyHashRedirect(): null {
  const navigate = useNavigate();

  useEffect(() => {
    const hash = window.location.hash;
    if (!hash || hash === "#/" || hash === "#") {
      return;
    }

    const raw = hash.slice(1);
    const [pathPart, queryPart = ""] = raw.split("?");
    const params = new URLSearchParams(queryPart);

    // Workspace routes must land on workspace.html, not the React SPA.
    if (pathPart === "/workspace" || pathPart.startsWith("/workspace/")) {
      window.location.replace("/workspace.html" + hash);
      return;
    }

    window.history.replaceState(null, "", window.location.pathname + window.location.search);

    if (pathPart.startsWith("/dashboard")) {
      navigate("/", { replace: true });
      return;
    }

    const workspaceMatch = pathPart.match(/^\/workspace\/([^/]+)\/([^/?]+)/);
    if (workspaceMatch) {
      const [, projectId, fileId] = workspaceMatch;
      const pageId = params.get("page-id");
      if (pageId) {
        // Duplicate path segments (file used as project) — let workspace.html repair it.
        if (projectId === fileId) {
          window.location.replace("/workspace.html" + hash);
          return;
        }
        void getProfile().then((profile) => {
            const teamId = profile["default-team-id"];
            if (teamId) {
              window.location.replace(workspaceURL(teamId, fileId, pageId));
            }
          });
      }
    }
  }, [navigate]);

  return null;
}
