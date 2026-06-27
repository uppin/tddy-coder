/**
 * Shared backend builder for Screen Sharing Cypress component tests.
 *
 * Returns an `InMemoryRpcBackend` pre-seeded with the RPCs that
 * `SessionsDrawerScreen` calls on startup. Callers chain additional
 * `.onUnary()` stubs for specific ScreenSharingService methods under test.
 *
 * @example
 * ```ts
 * const backend = aSessionsDrawerBackend([SESSION])
 *   .onUnary(ScreenSharingService.method.listTargets, () => ({ targets: [] }))
 *   .onUnary(ScreenSharingService.method.addTarget, (req) => ({
 *     target: { id: "t-001", label: req.label, host: req.host, port: req.port, protocol: req.protocol },
 *   }));
 *
 * mountWithRpc(<SessionsDrawerScreen />, backend);
 * ```
 */

import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { ConnectionService, type SessionEntry } from "../../../src/gen/connection_pb";

/**
 * Create an in-memory backend pre-seeded with all RPCs `SessionsDrawerScreen`
 * calls on startup. Callers add ScreenSharing or other test-specific stubs on top.
 */
export function aSessionsDrawerBackend(
  sessions: Partial<SessionEntry>[],
): InMemoryRpcBackend {
  return anInMemoryRpcBackend()
    .onUnary(ConnectionService.method.listSessions, () => ({ sessions }));
}
