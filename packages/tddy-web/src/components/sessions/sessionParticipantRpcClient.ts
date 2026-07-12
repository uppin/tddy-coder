/**
 * Build a `ConnectionService` client that targets a session's own LiveKit participant identity
 * (`daemon-<instanceId>-<sessionId>`) rather than the daemon participant (`daemon-<instanceId>`).
 *
 * Used for session-scoped RPCs (tools, terminal control, VNC, screen-sharing) on an attached
 * session. `DeleteSession` / `SignalSession` stay on the daemon participant client (daemon-direct).
 *
 * Changeset: `2026-07-12-fast-session-change`
 * Feature: `docs/ft/web/session-drawer.md#fast-session-change` (req 1)
 */

import { createClient, type Client, type Transport } from "@connectrpc/connect";
import { ConnectionService } from "../../gen/connection_pb";

/** The LiveKit identity a session's coder participant joins as. */
export function sessionParticipantIdentity(daemonInstanceId: string, sessionId: string): string {
  return `daemon-${daemonInstanceId}-${sessionId}`;
}

/**
 * Build a `ConnectionService` client over `room` targeting the session participant identity.
 *
 * `liveKitFactory` is the transport factory from `useLiveKitTransportFactory()`; `room` is the
 * session's attached LiveKit `Room`.
 */
export function buildSessionParticipantRpcClient(
  liveKitFactory: (room: unknown, targetIdentity: string) => Transport,
  room: unknown,
  sessionId: string,
  daemonInstanceId: string,
): Client<typeof ConnectionService> {
  const transport = liveKitFactory(room, sessionParticipantIdentity(daemonInstanceId, sessionId));
  return createClient(ConnectionService, transport);
}
