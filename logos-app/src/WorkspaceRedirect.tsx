import { useEffect } from "react";
import { useParams, useSearchParams } from "react-router-dom";
import { getProfile, workspaceURL } from "./api/client";

/**
 * Old React /workspace/* links go to the full Loogs editor.
 */
export function WorkspaceRedirect(): null {
  const { projectId, fileId } = useParams<{ projectId: string; fileId: string }>();
  const [searchParams] = useSearchParams();
  const pageId = searchParams.get("page-id") ?? "";

  useEffect(() => {
    if (!fileId || !pageId) return;
    void getProfile().then((profile) => {
      const teamId = profile["default-team-id"];
      if (teamId) {
        window.location.replace(workspaceURL(teamId, fileId, pageId));
      }
    });
  }, [projectId, fileId, pageId]);

  return null;
}
