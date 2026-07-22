import { useEffect, useMemo, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { AgentActivityRecord, ConnectionService } from "../../gen/connection_pb";

export interface UseSessionActivityResult {
  /** All known tool-call records, coalesced by `callId`, in first-seen order. */
  records: AgentActivityRecord[];
  /** True once the session has streamed at least one tool-call record. */
  hasActivity: boolean;
  /** How many known records have not yet been marked seen. */
  unreadCount: number;
  /** Mark every currently-known record as seen (clears the unread count for them). */
  markSeen: () => void;
}

/**
 * Subscribes to `ConnectionService.StreamSessionActivity` for one session and exposes the agent's
 * own tool calls to the Agent Activity pane. The server replays a coalesced snapshot then streams
 * live deltas; this hook additionally coalesces by `callId` so a later record (e.g. the terminal
 * row) supersedes the earlier `running` row for the same call while preserving first-seen order.
 *
 * Unread semantics: a record stays unread until `markSeen()` is called at/after its arrival. The
 * overlay calls `markSeen()` on open and while open, so records arriving while it is closed (or the
 * very first record before any open) surface as unread on the icon badge.
 *
 * The streaming-subscription and cancellation shape mirrors `useAgentChat` — a `cancelled` flag
 * plus a cleanup that stops iterating, swallowing the unmount AbortError.
 */
export function useSessionActivity(args: {
  sessionId: string;
  sessionToken: string;
  client: Client<typeof ConnectionService>;
}): UseSessionActivityResult {
  const { sessionId, sessionToken, client } = args;

  const [records, setRecords] = useState<AgentActivityRecord[]>([]);
  const [seenCallIds, setSeenCallIds] = useState<ReadonlySet<string>>(() => new Set());

  useEffect(() => {
    setRecords([]);
    let cancelled = false;

    const upsert = (record: AgentActivityRecord) => {
      setRecords((prev) => {
        const index = prev.findIndex((r) => r.callId === record.callId);
        if (index === -1) return [...prev, record];
        const next = prev.slice();
        next[index] = record;
        return next;
      });
    };

    (async () => {
      try {
        for await (const record of client.streamSessionActivity({
          sessionToken,
          sessionId,
          daemonInstanceId: "",
        })) {
          if (cancelled) break;
          upsert(record);
        }
      } catch (err) {
        // A stream aborted on unmount surfaces as an AbortError; ignore it. Any other error while
        // still mounted is left for the caller to observe via an empty/partial record set — the
        // pane simply shows what it has (no fallback fabrication).
        if (!cancelled) {
          console.debug("[useSessionActivity] stream error", err);
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [client, sessionId, sessionToken]);

  const markSeen = () => {
    setSeenCallIds(new Set(records.map((r) => r.callId)));
  };

  const unreadCount = useMemo(
    () => records.filter((r) => !seenCallIds.has(r.callId)).length,
    [records, seenCallIds],
  );

  return {
    records,
    hasActivity: records.length > 0,
    unreadCount,
    markSeen,
  };
}
