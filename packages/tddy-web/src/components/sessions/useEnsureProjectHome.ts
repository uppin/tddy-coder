import { useCallback } from "react";
import { Code, ConnectError, type Client } from "@connectrpc/connect";
import type { SessionService } from "../../gen/session_pb";
import type { StartSessionRequestInit } from "../../hooks/useSessionAttachments";
import { unservedService } from "../../rpc/rpcFailure";

/** What the screen does with the outcomes of an ensure beyond selecting the session it returns. */
export interface EnsureProjectHomeHandlers {
  /**
   * The session a start spec set main up with (or replaced it with), in place of `onEnsured`: a
   * newly started session is selected and attached the way a created one is.
   */
  onStarted?: (sessionId: string) => void;
  /**
   * The daemon has no main for the project (`NOT_FOUND`): none is recorded, or the recorded one no
   * longer exists and the daemon cleared it. The screen re-reads the registry and opens the setup
   * pane; the ensure then resolves, since the operator was sent somewhere to act on it.
   */
  onNoMain?: (projectId: string) => void;
}

/**
 * Calls `EnsureProjectHomeSession` for a project. `ensure` reaches the recorded main and hands its
 * id to `onEnsured` (the screen selects it); `start` sets main up — or replaces it — from the
 * New-session form's start spec. A refusal rejects with the daemon's reason, which the pinned entry
 * or the setup form shows (`#project-home` 4/7).
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-10-24-project-home-from-new-session-form.md
 */
export function useEnsureProjectHome(
  client: Client<typeof SessionService> | null,
  sessionToken: string,
  onEnsured: (sessionId: string) => void,
  { onStarted, onNoMain }: EnsureProjectHomeHandlers = {},
): {
  ensure: (projectId: string, daemonInstanceId: string) => Promise<void>;
  start: (projectId: string, daemonInstanceId: string, spec: StartSessionRequestInit) => Promise<void>;
} {
  const ensureSession = useCallback(
    async (projectId: string, daemonInstanceId: string, start?: StartSessionRequestInit) => {
      if (!client) throw new Error("not connected to a daemon");
      const response = await client.ensureProjectHomeSession({
        sessionToken,
        projectId,
        daemonInstanceId,
        start,
      });
      if (!response.session) throw new Error("the daemon returned no home session");
      return response.session.sessionId;
    },
    [client, sessionToken],
  );

  const ensure = useCallback(
    async (projectId: string, daemonInstanceId: string): Promise<void> => {
      let sessionId: string;
      try {
        sessionId = await ensureSession(projectId, daemonInstanceId);
      } catch (e: unknown) {
        if (onNoMain && isNoMain(e)) {
          onNoMain(projectId);
          return;
        }
        throw e;
      }
      onEnsured(sessionId);
    },
    [ensureSession, onEnsured, onNoMain],
  );

  const start = useCallback(
    async (projectId: string, daemonInstanceId: string, spec: StartSessionRequestInit) => {
      const sessionId = await ensureSession(projectId, daemonInstanceId, spec);
      (onStarted ?? onEnsured)(sessionId);
    },
    [ensureSession, onStarted, onEnsured],
  );

  return { ensure, start };
}

/** The daemon's "this project has no main" — not a daemon that does not serve the RPC at all. */
function isNoMain(error: unknown): boolean {
  return ConnectError.from(error).code === Code.NotFound && unservedService(error) === null;
}
