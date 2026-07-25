import { useCallback, useEffect, useState, useSyncExternalStore } from "react";
import { fromBinary } from "@bufbuild/protobuf";
import type { Client } from "@connectrpc/connect";
import { type ConnectionService, StreamMode } from "../../gen/connection_pb";
import { AcpAgentMessageSchema, ToolCallStatus } from "../../gen/tddy/acp/v1/acp_pb";
import { agentActivityRegistry } from "../sessions/agentActivityRegistry";
import { createAgentChunkMerger } from "./acpAgentMerge";
import type { ChatMessage, UseAgentChatResult } from "./useAgentChat";

/** Map an ACP `ToolCallStatus` onto the transcript's coarse status marker. Unspecified/pending/
 *  in-progress all read as "running"; a failed call reads "error"; a completed call "completed". */
function toolStatusOf(status: ToolCallStatus): "running" | "completed" | "error" {
  switch (status) {
    case ToolCallStatus.COMPLETED:
      return "completed";
    case ToolCallStatus.FAILED:
      return "error";
    default:
      return "running";
  }
}

/** The read-only transcript surface the Agent Activity overlay renders. Extends the shared
 *  {@link UseAgentChatResult} (so `AgentChatView` can render it interchangeably) with the overlay's
 *  icon/badge signals and the lazy-snapshot controls. Send/answer methods are inert — a replay is
 *  not interactive. */
export interface UseAcpReplayResult extends UseAgentChatResult {
  /** True once the count feed reports at least one persisted activity frame. */
  hasActivity: boolean;
  /** Activity frames counted since the overlay was last opened (drives the unread badge). */
  unreadCount: number;
  /** Mark the current count as seen (clears the unread badge). */
  markSeen: () => void;
  /** Open the heavy transcript snapshot for this session — call on first overlay open. Reuses the
   *  cached transcript on a later open (or a switch-back), so the snapshot stream opens at most once
   *  per delivered transcript. */
  loadSnapshot: () => void;
  /** True once the snapshot pull has delivered this session's transcript. */
  snapshotLoaded: boolean;
}

const NOOP_SEND = () => false;

/**
 * Subscribes to `ConnectionService.StreamAcpReplay` for one session in two phases, backed by the
 * module-level {@link agentActivityRegistry} so state survives a session switch:
 *
 * - a **count** feed (`COUNT_THEN_LIVE`), opened while the session is focused, whose frames carry
 *   `activity_count` (no transcript payload). This drives `hasActivity` and `unreadCount` cheaply,
 *   without pulling the full transcript.
 * - a **snapshot** feed (`SNAPSHOT_THEN_LIVE`), opened lazily by {@link UseAcpReplayResult.loadSnapshot}
 *   on the first overlay open. Its ACP `session_update` frames are projected into the read-only chat
 *   transcript and cached in the registry, so a switch away and back reuses it rather than re-pulling.
 *   A pull only counts as done once its frames actually land, so one that is cancelled mid-flight
 *   (a remount, a session switch) is retried instead of caching an empty transcript.
 *
 * Frame projection mirrors the live ACP path: `agent_message_chunk` text merges into agent bubbles
 * (via {@link createAgentChunkMerger}, finalized per recorded chunk so discrete chunks stay separate);
 * `tool_call` becomes a tool entry carrying the server-enriched `title`, a coarse status, and its
 * `raw_input`/`raw_output` (for the detail dialog), coalesced by `tool_call_id`; `user_message_chunk`
 * a user bubble; `agent_thought_chunk` a goal bubble. Each entry's timestamp is the frame's
 * `SessionNotification.timestamp_unix_ms`, so elapsed badges reflect the recorded timeline.
 */
export function useAcpReplay(args: {
  sessionId: string;
  sessionToken: string;
  client: Client<typeof ConnectionService>;
}): UseAcpReplayResult {
  const { sessionId, sessionToken, client } = args;

  const state = useSyncExternalStore(
    (listener) => agentActivityRegistry.subscribe(listener),
    () => agentActivityRegistry.get(sessionId),
  );

  const count = state?.count ?? 0;
  const seenCount = state?.seenCount ?? 0;
  const messages = state?.messages ?? [];
  const snapshotLoaded = state?.snapshotLoaded ?? false;

  // Both stream effects key on `client`, so a genuine routing change (daemon-direct → session-scoped
  // once the session's room connects) re-subscribes over the new transport. That is only safe because
  // the client's *identity* is stable at the source: hosts build it inline while rendering
  // (`buildSessionClient?.() ?? client` in `SessionMainPane`) but resolve the build through
  // `SessionClientCache`, so an unchanged route hands back the same client and a mere re-render
  // cannot tear a subscription down mid-flight.

  // The count feed runs while the session is focused: cheap frames carrying only `activity_count`.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        for await (const frame of client.streamAcpReplay({
          sessionToken,
          sessionId,
          daemonInstanceId: "",
          mode: StreamMode.COUNT_THEN_LIVE,
        })) {
          if (cancelled) break;
          agentActivityRegistry.setCount(sessionId, Number(frame.activityCount));
        }
      } catch (err) {
        // A stream aborted on unmount surfaces as an AbortError; ignore it (no fallback fabrication).
        if (!cancelled) console.debug("[useAcpReplay] count stream error", err);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [client, sessionId, sessionToken]);

  // Which session's snapshot has been requested via `loadSnapshot`. Held per-hook (not per-session)
  // so switching away to an unvisited session does NOT eagerly open its snapshot.
  const [snapshotSession, setSnapshotSession] = useState<string | null>(null);
  const loadSnapshot = useCallback(() => setSnapshotSession(sessionId), [sessionId]);

  // The snapshot feed: opened lazily once `loadSnapshot` targets the current session, and only when
  // the registry has not already loaded it (a switch-back reuses the cached transcript).
  useEffect(() => {
    if (snapshotSession !== sessionId) return;
    if (agentActivityRegistry.get(sessionId)?.snapshotLoaded) return;

    let cancelled = false;
    const merger = createAgentChunkMerger();
    const acc: ChatMessage[] = [];
    // Position in `acc` of the entry for each seen tool_call_id, so a later frame carrying an id we
    // already rendered refines that same entry instead of appending a duplicate.
    const toolIndexById = new Map<string, number>();
    let toolKey = 0;
    let userKey = 0;
    let goalKey = 0;

    (async () => {
      try {
        for await (const frame of client.streamAcpReplay({
          sessionToken,
          sessionId,
          daemonInstanceId: "",
          mode: StreamMode.SNAPSHOT_THEN_LIVE,
        })) {
          if (cancelled) break;
          // The pull counts as done from its first delivered frame onwards — marking it earlier (at
          // subscribe time) would leave a pull cancelled before any frame landed cached as an empty
          // transcript that no later open would refill.
          agentActivityRegistry.markSnapshotLoaded(sessionId);
          const msg = fromBinary(AcpAgentMessageSchema, frame.acpAgentMessage);
          if (msg.msg.case !== "sessionUpdate") continue;
          const notification = msg.msg.value;
          const at = Number(notification.timestampUnixMs);
          const update = notification.update?.update;
          if (!update) continue;

          if (update.case === "agentMessageChunk") {
            const block = update.value.content?.block;
            if (block?.case === "text") {
              merger.appendChunk(acc, block.value.text, at);
              // A replayed chunk is a complete recorded event: finalize it so the next chunk opens a
              // new bubble instead of concatenating onto this one.
              merger.finalize(acc, at);
            }
          } else if (update.case === "toolCall") {
            // The server emits a tool call as it progresses (e.g. in-progress then completed) under
            // one tool_call_id. Coalesce by id: a repeat refines the existing entry's label/status/
            // timestamp/payload in place (keeping its key + position). Only non-empty ids coalesce;
            // a missing id always opens a new entry.
            const id = update.value.toolCallId?.value ?? "";
            const existingIndex = id ? toolIndexById.get(id) : undefined;
            const rawInput = update.value.rawInput ?? "";
            const rawOutput = update.value.rawOutput ?? "";
            if (existingIndex !== undefined) {
              acc[existingIndex] = {
                ...acc[existingIndex],
                text: update.value.title,
                at,
                toolStatus: toolStatusOf(update.value.status),
                rawInput,
                rawOutput,
              };
            } else {
              if (id) toolIndexById.set(id, acc.length);
              acc.push({
                key: `tool-${toolKey++}`,
                text: update.value.title,
                from: "tool",
                at,
                toolStatus: toolStatusOf(update.value.status),
                rawInput,
                rawOutput,
              });
            }
          } else if (update.case === "userMessageChunk") {
            const block = update.value.content?.block;
            if (block?.case === "text") {
              acc.push({ key: `user-${userKey++}`, text: block.value.text, from: "user", at });
            }
          } else if (update.case === "agentThoughtChunk") {
            // tddy convention: the thought channel carries the workflow goal → "goal" bubble.
            const block = update.value.content?.block;
            if (block?.case === "text") {
              acc.push({ key: `goal-${goalKey++}`, text: block.value.text, from: "goal", at });
            }
          }
          // tool_call_update / plan carry no additional bubble; ignored on purpose.

          if (!cancelled) agentActivityRegistry.setMessages(sessionId, acc.slice());
        }
      } catch (err) {
        // A stream aborted on unmount surfaces as an AbortError; ignore it. Any other error while
        // still mounted leaves the transcript showing what it has (no fallback fabrication).
        if (!cancelled) console.debug("[useAcpReplay] snapshot stream error", err);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [client, sessionId, sessionToken, snapshotSession]);

  const markSeen = useCallback(() => {
    agentActivityRegistry.markSeen(sessionId);
  }, [sessionId]);

  const unreadCount = Math.max(0, count - seenCount);

  return {
    messages,
    elicitations: [],
    sendPrompt: NOOP_SEND,
    pendingQuestion: null,
    answerSelect: NOOP_SEND,
    answerOther: NOOP_SEND,
    answerMultiSelect: NOOP_SEND,
    streamError: null,
    sendError: null,
    workflowError: null,
    hasActivity: count > 0,
    unreadCount,
    markSeen,
    loadSnapshot,
    snapshotLoaded,
  };
}
