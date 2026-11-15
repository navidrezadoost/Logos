import type { PersistedDocument } from "./persist";
import type { WorkspaceSession } from "./workspace-session";

export interface WorkspaceLifecycleHandle {
  stop(): void;
}

export interface WorkspaceSyncHandle extends WorkspaceLifecycleHandle {
  start(): void;
}

export interface WorkspaceServerFile {
  id: string;
  data?: { pages?: unknown };
}

export interface OpenWorkspaceDependencies {
  readPersistedDocument(documentId: string): Promise<PersistedDocument | null>;
  applyPersistedDocument(documentId: string, persisted: PersistedDocument, canApply: () => boolean): boolean;
  getFile(fileId: string): Promise<WorkspaceServerFile>;
  associateServerIdentity(fileId: string, returnedId: string, pageIds: unknown[], canApply: () => boolean): boolean;
  selectPage(pageId: string, canApply: () => boolean): void;
  startPersistence(documentId: string, canApply: () => boolean): WorkspaceLifecycleHandle;
  createSyncManager(documentId: string, canApply: () => boolean): WorkspaceSyncHandle;
  onReady(): void;
  onError(error: unknown): void;
}

/**
 * Run the async opening sequence for one route attempt. Every await is followed
 * by an ownership check; state application functions receive the same guard so
 * they can reject stale work immediately at the mutation boundary.
 */
export function openWorkspaceSession(
  session: WorkspaceSession,
  pageId: string | null,
  dependencies: OpenWorkspaceDependencies,
): () => void {
  let persistence: WorkspaceLifecycleHandle | null = null;
  let sync: WorkspaceSyncHandle | null = null;
  let disposed = false;

  const dispose = () => {
    if (disposed) return;
    disposed = true;
    session.dispose();
    persistence?.stop();
    sync?.stop();
    persistence = null;
    sync = null;
  };

  void (async () => {
    try {
      const persisted = await dependencies.readPersistedDocument(session.documentId);
      if (!session.isCurrent()) return;
      const restored = persisted !== null && dependencies.applyPersistedDocument(
        session.documentId,
        persisted,
        session.isCurrent,
      );
      if (!session.isCurrent()) return;

      if (pageId) dependencies.selectPage(pageId, session.isCurrent);

      if (restored) {
        try {
          const file = await dependencies.getFile(session.documentId);
          if (!session.isCurrent()) return;
          if (Array.isArray(file.data?.pages)) {
            dependencies.associateServerIdentity(
              session.documentId,
              file.id,
              file.data.pages,
              session.isCurrent,
            );
          }
        } catch {
          // A cached workspace remains usable offline. The route itself is not
          // used to bind a server identity; only a matching response can do so.
          if (!session.isCurrent()) return;
        }
      }

      if (!session.isCurrent()) return;
      persistence = dependencies.startPersistence(session.documentId, session.isCurrent);
      if (!session.isCurrent()) return;
      sync = dependencies.createSyncManager(session.documentId, session.isCurrent);
      if (!session.isCurrent()) return;
      sync.start();
      dependencies.onReady();
    } catch (error) {
      if (session.isCurrent()) dependencies.onError(error);
    }
  })();

  return dispose;
}
