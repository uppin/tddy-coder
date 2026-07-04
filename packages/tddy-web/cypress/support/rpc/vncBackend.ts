/**
 * Shared backend builder for VNC-related Cypress component tests.
 *
 * Returns an `InMemoryRpcBackend` with the minimum stubs required for
 * `SessionsDrawerScreen` to load and open the inspector without errors.
 * Callers chain additional `.onUnary()` stubs for the specific VNC methods
 * under test.
 *
 * @example
 * ```ts
 * const backend = aSessionsDrawerBackend([SESSION])
 *   .onUnary(VncService.method.listVncTargets, () => ({ targets: [] }))
 *   .onUnary(VncService.method.addVncTarget, (req) => ({
 *     target: { id: "t-001", label: req.label, host: req.host, port: req.port },
 *   }));
 *
 * cy.mountWithRpc(<SessionsDrawerScreen />, backend);
 * ```
 */

import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { AuthService } from "../../../src/gen/auth_pb";
import { ConnectionService, type SessionEntry } from "../../../src/gen/connection_pb";
import { aGitHubUser } from "./responses";

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/**
 * Create an in-memory backend pre-seeded with all RPCs `SessionsDrawerScreen`
 * calls on startup.  Callers add VNC or other test-specific stubs on top.
 */
export function aSessionsDrawerBackend(
  sessions: Partial<SessionEntry>[],
): InMemoryRpcBackend {
  return anInMemoryRpcBackend()
    .onUnary(AuthService.method.getAuthStatus, () => ({ authenticated: true, user: aGitHubUser() }))
    .onUnary(ConnectionService.method.listSessions, () => ({ sessions }));
}
