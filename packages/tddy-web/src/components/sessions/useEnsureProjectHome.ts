import { useCallback, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { SessionService } from "../../gen/session_pb";

/**
 * Calls `EnsureProjectHomeSession` for a project and hands the resulting session id to
 * `onEnsured` (the screen selects it). A refusal is kept per project for the pinned entry to show,
 * and rethrown to the caller (`#project-home` 4/7).
 */
export function useEnsureProjectHome(
  client: Client<typeof SessionService> | null,
  sessionToken: string,
  onEnsured: (sessionId: string) => void,
): {
  ensure: (projectId: string, daemonInstanceId: string) => Promise<void>;
  errorByProject: ReadonlyMap<string, string>;
} {
  const [errorByProject, setErrorByProject] = useState<ReadonlyMap<string, string>>(new Map());

  const setError = useCallback((projectId: string, message: string | null) => {
    setErrorByProject((prev) => {
      const next = new Map(prev);
      if (message === null) next.delete(projectId);
      else next.set(projectId, message);
      return next;
    });
  }, []);

  const ensure = useCallback(
    async (projectId: string, daemonInstanceId: string): Promise<void> => {
      if (!client) throw new Error("not connected to a daemon");
      setError(projectId, null);
      try {
        const response = await client.ensureProjectHomeSession({
          sessionToken,
          projectId,
          daemonInstanceId,
        });
        if (!response.session) throw new Error("the daemon returned no home session");
        onEnsured(response.session.sessionId);
      } catch (e) {
        setError(projectId, e instanceof Error ? e.message : String(e));
        throw e;
      }
    },
    [client, sessionToken, onEnsured, setError],
  );

  return { ensure, errorByProject };
}
