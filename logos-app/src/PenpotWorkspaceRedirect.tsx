import { useEffect } from "react";
import { useParams, useSearchParams } from "react-router-dom";
import { getProfile, penpotWorkspaceURL } from "./api/client";

/**
 * Old React /workspace/* links → full Penpot editor at /workspace.html#/workspace?…
 */
export function PenpotWorkspaceRedirect(): null {
  const { projectId, fileId } = useParams<{ projectId: string; fileId: string }>();
  const [searchParams] = useSearchParams();
  const pageId = searchParams.get("page-id") ?? "";

  useEffect(() => {
    if (!fileId || !pageId) return;
    void getProfile().then((profile) => {
      const teamId = profile["default-team-id"];
      if (teamId) {
        window.location.replace(penpotWorkspaceURL(teamId, fileId, pageId));
      }
    });
  }, [projectId, fileId, pageId]);

  return null;
}
