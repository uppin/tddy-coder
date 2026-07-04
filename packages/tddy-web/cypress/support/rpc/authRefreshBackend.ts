/**
 * In-memory `auth.AuthService` (+ minimal `connection.ConnectionService`) backend for testing the
 * shared session-token refresh lifecycle (`AuthProvider`/`useAuthContext`).
 *
 * `RefreshSession` is a pure function of the sent token (`<sent-token>-refreshed`) rather than a
 * call-counter — deterministic regardless of how many independent callers happen to refresh the
 * same starting token, so tests aren't sensitive to unmigrated legacy `useAuth()` call sites (e.g.
 * `SelectedDaemonProvider`) also refreshing in the background. `ListSessions` returns an empty
 * list; tests only care about the `sessionToken` field it was sent.
 */

import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { AuthService } from "../../../src/gen/auth_pb";
import { ConnectionService } from "../../../src/gen/connection_pb";
import { aGitHubUser } from "./responses";

export function anAuthRefreshBackend(): InMemoryRpcBackend {
  return anInMemoryRpcBackend()
    .implement(AuthService, {
      getAuthStatus: async () => ({ authenticated: true, user: aGitHubUser() }),
      refreshSession: async (req) => ({
        sessionToken: `${req.sessionToken}-refreshed`,
        user: aGitHubUser(),
      }),
    })
    .implement(ConnectionService, {
      listSessions: async () => ({ sessions: [] }),
    });
}
