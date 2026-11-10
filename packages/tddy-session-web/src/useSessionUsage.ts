/**
 * Owns the daemon's `ActivityService.StreamSessionUsage` server stream for the Session Inspector
 * "Usage" tab and keeps the latest per-conversation token-usage snapshot.
 *
 * The daemon streams the full cumulative snapshot (never a delta) on subscribe and again each time
 * it changes; each one replaces the held snapshot. The stream is reached the way every other
 * Inspector tab reaches the daemon — its `ActivityService` client — so a session with no LiveKit
 * room (a terminal session, a host reached over HTTP) has usage too.
 *
 * Changeset: `usage-binding-daemon-stream` (`#usage-binding 2/5`)
 * PRD: `docs/ft/web/session-usage-inspector.md`
 */

import { useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { ActivityService } from "tddy-client-web/gen/activity_pb";
import { emptyUsage, type ConversationUsage } from "./sessionUsage";

/** The latest snapshot, and why the stream is not delivering (`null` while it is). */
export interface SessionUsageStream {
  usage: ConversationUsage[];
  /**
   * Why the stream stopped, ready to render; the last snapshot stays in `usage`. Cleared when a
   * snapshot arrives on a new subscription. `null` while there is no stream to fail.
   */
  error: string | null;
}

/**
 * Subscribes to `sessionId`'s token-usage stream on `client` and returns the latest snapshot, one
 * entry per conversation. `usage` is `emptyUsage()` until the first snapshot arrives (or forever,
 * if the session never reports usage); a stream that fails is reported in `error` instead of
 * looking like a session that reported nothing. No `client` means no stream to fail.
 */
export function useSessionUsage(
  client: Client<typeof ActivityService> | undefined,
  sessionToken: string,
  sessionId: string,
): SessionUsageStream {
  // TODO(daemon-stream): implement — `client.streamSessionUsage({ sessionToken, sessionId,
  // daemonInstanceId: "" })`, each snapshot mapped to `ConversationUsage` and replacing the last.
  void client;
  void sessionToken;
  void sessionId;
  const [usage] = useState<ConversationUsage[]>(emptyUsage);
  const [error] = useState<string | null>(null);
  return { usage, error };
}
