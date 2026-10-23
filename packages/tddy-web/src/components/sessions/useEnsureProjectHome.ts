import { useCallback } from "react";
import type { Client } from "@connectrpc/connect";
import type { SessionService } from "../../gen/session_pb";

/**
 * Calls `EnsureProjectHomeSession` for a project and hands the resulting session id to
 * `onEnsured` (the screen selects it). A refusal rejects with the daemon's reason, which the pinned
 * entry shows (`#project-home` 4/7).
 */
export function useEnsureProjectHome(
  client: Client<typeof SessionService> | null,
  sessionToken: string,
  onEnsured: (sessionId: string) => void,
): { ensure: (projectId: string, daemonInstanceId: string) => Promise<void> } {
  const ensure = useCallback(
    async (projectId: string, daemonInstanceId: string): Promise<void> => {
      if (!client) throw new Error("not connected to a daemon");
      const response = await client.ensureProjectHomeSession({
        sessionToken,
        projectId,
        daemonInstanceId,
      });
      if (!response.session) throw new Error("the daemon returned no home session");
      onEnsured(response.session.sessionId);
    },
    [client, sessionToken, onEnsured],
  );

  return { ensure };
}
