import { useEffect, useMemo, useState } from "react";
import { fromBinary } from "@bufbuild/protobuf";
import type { Client } from "@connectrpc/connect";
import { type ConnectionService, StreamMode } from "../../gen/connection_pb";
import { AcpAgentMessageSchema, ToolCallStatus } from "../../gen/tddy/acp/v1/acp_pb";
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
 *  icon/badge signals. Send/answer methods are inert — a replay is not interactive. */
export interface UseAcpReplayResult extends UseAgentChatResult {
  /** True once the replay has produced at least one transcript entry. */
  hasActivity: boolean;
  /** How many entries have not yet been marked seen (drives the unread badge). */
  unreadCount: number;
  /** Mark every currently-known entry as seen (clears the unread count for them). */
  markSeen: () => void;
}

const NOOP_SEND = () => false;

/**
 * Subscribes to `ConnectionService.StreamAcpReplay` for one session and projects the agent's ACP
 * conversation (`AcpAgentMessage` frames) into a read-only chat transcript for the Agent Activity
 * overlay. The server replays the coalesced history then tails live (`SNAPSHOT_THEN_LIVE`), so the
 * transcript is populated on open for live and dormant sessions alike.
 *
 * Each frame carries the protobuf bytes of an `AcpAgentMessage`; only the `session_update` variant
 * is projected. `agent_message_chunk` text merges into agent bubbles (via the shared
 * {@link createAgentChunkMerger}, finalized per recorded chunk so discrete chunks stay separate
 * bubbles); `tool_call` becomes a tool entry carrying the server-enriched `title` and a coarse
 * status; `user_message_chunk` a user bubble; `agent_thought_chunk` a goal bubble. Every entry's
 * timestamp is the frame's `SessionNotification.timestamp_unix_ms` (wall-clock at record time), so
 * the transcript's elapsed badges reflect the recorded timeline, not render time.
 *
 * The streaming-subscription and cancellation shape mirrors `useSessionActivity` — a `cancelled`
 * flag plus a cleanup that stops iterating, swallowing the unmount AbortError. Unread semantics also
 * mirror it: an entry stays unread until `markSeen()` is called at/after its arrival.
 */
export function useAcpReplay(args: {
  sessionId: string;
  sessionToken: string;
  client: Client<typeof ConnectionService>;
}): UseAcpReplayResult {
  const { sessionId, sessionToken, client } = args;

  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [seenKeys, setSeenKeys] = useState<ReadonlySet<string>>(() => new Set());

  useEffect(() => {
    setMessages([]);
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
            // timestamp in place (keeping its key + position), mirroring how the agent-activity log
            // coalesces by call_id. Only non-empty ids coalesce; a missing id always opens a new entry.
            const id = update.value.toolCallId?.value ?? "";
            const existingIndex = id ? toolIndexById.get(id) : undefined;
            if (existingIndex !== undefined) {
              acc[existingIndex] = {
                ...acc[existingIndex],
                text: update.value.title,
                at,
                toolStatus: toolStatusOf(update.value.status),
              };
            } else {
              if (id) toolIndexById.set(id, acc.length);
              acc.push({
                key: `tool-${toolKey++}`,
                text: update.value.title,
                from: "tool",
                at,
                toolStatus: toolStatusOf(update.value.status),
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

          if (!cancelled) setMessages(acc.slice());
        }
      } catch (err) {
        // A stream aborted on unmount surfaces as an AbortError; ignore it. Any other error while
        // still mounted leaves the transcript showing what it has (no fallback fabrication).
        if (!cancelled) {
          console.debug("[useAcpReplay] stream error", err);
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [client, sessionId, sessionToken]);

  const markSeen = () => {
    setSeenKeys(new Set(messages.map((m) => m.key)));
  };

  const unreadCount = useMemo(
    () => messages.filter((m) => !seenKeys.has(m.key)).length,
    [messages, seenKeys],
  );

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
    hasActivity: messages.length > 0,
    unreadCount,
    markSeen,
  };
}
