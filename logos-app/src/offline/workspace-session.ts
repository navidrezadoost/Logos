/**
 * Synchronous ownership for the one shared React document store.
 *
 * A workspace session is an attempt, not just a document ID: A → B → A creates
 * three distinct generations. Async callers must test `isCurrent()` at the
 * point where they apply results or start a side effect.
 */
export interface WorkspaceSession {
  readonly documentId: string;
  readonly routeKey: string;
  readonly generation: number;
  isCurrent(): boolean;
  dispose(): void;
}

let nextGeneration = 0;
let activeSession: WorkspaceSession | null = null;

export function beginWorkspaceSession(documentId: string, routeKey: string): WorkspaceSession {
  const generation = ++nextGeneration;
  let disposed = false;
  const session: WorkspaceSession = {
    documentId,
    routeKey,
    generation,
    isCurrent: () => !disposed && activeSession === session,
    dispose: () => {
      disposed = true;
      if (activeSession === session) activeSession = null;
    },
  };
  if (activeSession) activeSession.dispose();
  activeSession = session;
  return session;
}

export function getActiveWorkspaceSession(): WorkspaceSession | null {
  return activeSession?.isCurrent() ? activeSession : null;
}
