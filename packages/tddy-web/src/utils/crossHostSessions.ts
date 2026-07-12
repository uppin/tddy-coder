/**
 * Helpers for the sessions drawer's cross-host aggregation
 * (docs/ft/web/session-drawer.md § Cross-Host Active Sessions).
 *
 * A session that currently has a live LiveKit participant shows regardless of the selected host.
 * Liveness comes for free from **LiveKit participant presence**: a session's coder process joins the
 * shared common room as `daemon-<instanceId>-<sessionId>` (or `daemon-<sessionId>` on a single
 * daemon) and the LiveKit SDK keeps that participant alive while the process lives. So the active
 * rows are derived directly from the common-room participants — no per-host `ListSessions` fan-out.
 *
 * The drawer is the union of: every session on the **selected host** (from its `ListSessions`,
 * active or not) ∪ every session with a **live participant** on any host (from the room).
 */

import { create } from "@bufbuild/protobuf";
import { SessionEntrySchema, type SessionEntry } from "../gen/connection_pb";

/** A live session observed as a coder participant in the common room. */
export interface SessionParticipant {
  readonly sessionId: string;
  /** Owning daemon instance id, parsed from the identity; empty for a single-daemon `daemon-<id>`. */
  readonly owningInstanceId: string;
}

const SESSION_ID_UUID = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

/**
 * Parse a session coder participant identity into `{ sessionId, owningInstanceId }`, or `null` when
 * the identity is not a session participant. Session identities are `daemon-<instanceId>-<sessionId>`
 * (multi-host) or `daemon-<sessionId>` (single daemon), where `<sessionId>` is a UUID. This excludes
 * daemon RPC identities (`daemon-<instanceId>`, no trailing UUID), daemon advertisements (bare
 * instance id), and browsers (`web-…`).
 */
export function parseSessionParticipantIdentity(identity: string): SessionParticipant | null {
  if (!identity.startsWith("daemon-")) return null;
  const body = identity.slice("daemon-".length);
  const match = body.match(SESSION_ID_UUID);
  if (!match) return null;
  const sessionId = match[0];
  // Everything before the trailing `<sessionId>` is `<instanceId>-` (or empty for `daemon-<id>`).
  const owningInstanceId = body.slice(0, body.length - sessionId.length).replace(/-$/, "");
  return { sessionId, owningInstanceId };
}

/**
 * The live sessions observed among `participants`, de-duplicated by session id (first-seen wins).
 * Mirrors `daemonHostsFromParticipants` for daemons.
 */
export function sessionParticipantsFromParticipants(
  participants: ReadonlyArray<{ identity: string }>,
): SessionParticipant[] {
  const seen = new Set<string>();
  const sessions: SessionParticipant[] = [];
  for (const p of participants) {
    const parsed = parseSessionParticipantIdentity(p.identity);
    if (!parsed || seen.has(parsed.sessionId)) continue;
    seen.add(parsed.sessionId);
    sessions.push(parsed);
  }
  return sessions;
}

/**
 * The host a session belongs to: its `daemonInstanceId` when set, else `fallbackInstanceId` (the
 * selected host — legacy local daemons and synthesized rows on the selected host carry an empty id).
 */
export function owningHostForSession(session: SessionEntry, fallbackInstanceId: string): string {
  return session.daemonInstanceId.trim() || fallbackInstanceId;
}

/**
 * Union of the selected host's sessions and the live cross-host sessions. Selected-host entries
 * (which carry full metadata from `ListSessions`) win; a live participant not already present is
 * added as a minimal synthesized row owned by its host — its owner drives interaction routing and
 * the owning-host badge, and its label falls back to the short session id until metadata arrives.
 */
export function mergeActiveAndFetchedSessions(
  selectedHostSessions: SessionEntry[],
  activeParticipants: ReadonlyArray<SessionParticipant>,
  selectedInstanceId: string,
): SessionEntry[] {
  const byId = new Map<string, SessionEntry>();
  for (const s of selectedHostSessions) byId.set(s.sessionId, s);
  for (const p of activeParticipants) {
    if (byId.has(p.sessionId)) continue;
    byId.set(
      p.sessionId,
      create(SessionEntrySchema, {
        sessionId: p.sessionId,
        daemonInstanceId: p.owningInstanceId || selectedInstanceId,
        isActive: true,
      }),
    );
  }
  return Array.from(byId.values());
}
