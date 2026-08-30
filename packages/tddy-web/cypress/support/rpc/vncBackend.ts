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

import type { MessageInitShape } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { AuthService } from "../../../src/gen/auth_pb";
import {
  ConnectionService,
  type SessionContextDocSchema,
  type SessionEntry,
} from "../../../src/gen/connection_pb";
import { aGitHubUser } from "./responses";

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/**
 * One session as a scenario states it: whichever `SessionEntry` fields it is about, and nothing else.
 *
 * `contextDocs` is loosened to the message's *init* shape so a scenario can name the two fields it
 * cares about (`relativePath`, `exists`) without constructing a whole `SessionContextDoc` — the
 * router fills the rest with the same defaults a daemon that never wrote them would send.
 */
export type SessionEntryFixture = Partial<Omit<SessionEntry, "contextDocs">> & {
  contextDocs?: MessageInitShape<typeof SessionContextDocSchema>[];
};

/**
 * Create an in-memory backend pre-seeded with all RPCs `SessionsDrawerScreen`
 * calls on startup.  Callers add VNC or other test-specific stubs on top.
 */
export function aSessionsDrawerBackend(
  sessions: SessionEntryFixture[],
): InMemoryRpcBackend {
  return anInMemoryRpcBackend()
    .onUnary(AuthService.method.getAuthStatus, () => ({ authenticated: true, user: aGitHubUser() }))
    .onUnary(ConnectionService.method.listSessions, () => ({ sessions }));
}
