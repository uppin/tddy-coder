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
  // TODO(home-session): implement
  void client;
  void sessionToken;
  void onEnsured;
  return {
    ensure: () => Promise.reject(new Error("EnsureProjectHomeSession is not wired")),
    errorByProject: new Map(),
  };
}
