/**
 * Projection of `StreamAcpReplay` / `GetAcpReplayPage` frames into read-only transcript entries.
 *
 * This used to live inline in `useAcpReplay`'s stream effect, one frame at a time. Paging backwards
 * needs it as a function: a whole page arrives at once from `GetAcpReplayPage` and has to become
 * entries before it can be prepended above the loaded range.
 *
 * Because two pages end up in **one** rendered list, every projection is scoped by the absolute
 * transcript position (`firstSeq`) of the page it belongs to. React reconciles the prepended page
 * against the range already on screen by key, so a collision between two pages would silently drop or
 * reuse a rendered row.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-02-activities-tail-first-autoscroll.md § Client contract.
 */

import {
  ToolCallStatus,
  type AcpAgentMessage,
  type ContentChunk,
  type ToolCall,
} from "../../gen/tddy/acp/v1/acp_pb";
import { createAgentChunkMerger } from "./acpAgentMerge";
import type { ChatMessage } from "./useAgentChat";

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

/**
 * The state one page-scoped projection carries from frame to frame. Every appender below reads and
 * advances it rather than re-deriving anything: the key counters have to keep counting across
 * frames, and a tool call is only recognised as one already rendered because `toolIndexById`
 * remembers where it went.
 */
interface ProjectionContext {
  /** Absolute transcript position of the page's oldest frame. Scopes every key the page emits, so
   *  two pages in one rendered list cannot collide. */
  readonly firstSeq: number;
  readonly merger: ReturnType<typeof createAgentChunkMerger>;
  /** Position in `entries` of the row for each seen tool_call_id, so a later frame carrying an id we
   *  already rendered refines that same row instead of appending a duplicate. */
  readonly toolIndexById: Map<string, number>;
  toolKey: number;
  userKey: number;
  goalKey: number;
}

/** `agent_message_chunk`: text merges into agent bubbles, as on the live ACP path. */
function appendAgentMessageChunk(
  ctx: ProjectionContext,
  entries: ChatMessage[],
  chunk: ContentChunk,
  at: number,
): void {
  const block = chunk.content?.block;
  if (block?.case !== "text") return;
  ctx.merger.appendChunk(entries, block.value.text, at);
  // A replayed chunk is a complete recorded event: finalize it so the next chunk opens a new
  // bubble instead of concatenating onto this one.
  ctx.merger.finalize(entries, at);
}

/** `tool_call`: a tool entry carrying the server-enriched `title` and a coarse status, coalesced by
 *  `tool_call_id` so a call refined from running to completed keeps one row. */
function appendToolCall(
  ctx: ProjectionContext,
  entries: ChatMessage[],
  call: ToolCall,
  at: number,
): void {
  // The `tool_call_id` is persisted onto the entry: the streamed frame carries no
  // `raw_input`/`raw_output` (the hosts strip them), so the detail dialog fetches the bodies by
  // id instead of reading them off the message. It is written only when non-empty — an id-less
  // frame leaves `toolCallId` `undefined` (as its optionality states) rather than carrying `""`,
  // which would send the dialog looking up the empty id and have the host answer `NOT_FOUND`.
  const id = call.toolCallId?.value ?? "";
  const idField = id ? { toolCallId: id } : {};
  const existingIndex = id ? ctx.toolIndexById.get(id) : undefined;
  if (existingIndex !== undefined) {
    entries[existingIndex] = {
      ...entries[existingIndex],
      text: call.title,
      at,
      toolStatus: toolStatusOf(call.status),
      ...idField,
    };
    return;
  }
  if (id) ctx.toolIndexById.set(id, entries.length);
  entries.push({
    key: `tool-${ctx.firstSeq}-${ctx.toolKey++}`,
    text: call.title,
    from: "tool",
    at,
    toolStatus: toolStatusOf(call.status),
    ...idField,
  });
}

/** `user_message_chunk`: a user bubble. */
function appendUserMessageChunk(
  ctx: ProjectionContext,
  entries: ChatMessage[],
  chunk: ContentChunk,
  at: number,
): void {
  const block = chunk.content?.block;
  if (block?.case !== "text") return;
  entries.push({
    key: `user-${ctx.firstSeq}-${ctx.userKey++}`,
    text: block.value.text,
    from: "user",
    at,
  });
}

/** `agent_thought_chunk`: a goal bubble — tddy convention is that the thought channel carries the
 *  workflow goal. */
function appendAgentThoughtChunk(
  ctx: ProjectionContext,
  entries: ChatMessage[],
  chunk: ContentChunk,
  at: number,
): void {
  const block = chunk.content?.block;
  if (block?.case !== "text") return;
  entries.push({
    key: `goal-${ctx.firstSeq}-${ctx.goalKey++}`,
    text: block.value.text,
    from: "goal",
    at,
  });
}

/** A running projection over one page of the transcript (or over the live tail that continues it). */
export interface ReplayProjector {
  /** Fold one ACP frame into the projection and return the entries produced so far, oldest-first.
   *  The returned array is a fresh snapshot, so a caller may hand it straight to a store. */
  append(message: AcpAgentMessage): ChatMessage[];
}

/**
 * Open a projection over the page whose oldest frame sits at absolute transcript position
 * `firstSeq`. Frames are folded in one at a time — the shape the live tail delivers them in — and
 * each call returns the whole accumulated page, so a live frame extends the transcript rather than
 * replacing it.
 *
 * Frame handling mirrors the live ACP path, one appender per update case above.
 * `tool_call_update` / `plan` carry no additional bubble.
 */
export function createReplayProjector(firstSeq: number): ReplayProjector {
  const ctx: ProjectionContext = {
    firstSeq,
    merger: createAgentChunkMerger(`agent-${firstSeq}`),
    toolIndexById: new Map<string, number>(),
    toolKey: 0,
    userKey: 0,
    goalKey: 0,
  };
  const entries: ChatMessage[] = [];

  return {
    append(message: AcpAgentMessage): ChatMessage[] {
      if (message.msg.case !== "sessionUpdate") return entries.slice();
      const notification = message.msg.value;
      const at = Number(notification.timestampUnixMs);
      const update = notification.update?.update;
      if (!update) return entries.slice();

      switch (update.case) {
        case "agentMessageChunk":
          appendAgentMessageChunk(ctx, entries, update.value, at);
          break;
        case "toolCall":
          appendToolCall(ctx, entries, update.value, at);
          break;
        case "userMessageChunk":
          appendUserMessageChunk(ctx, entries, update.value, at);
          break;
        case "agentThoughtChunk":
          appendAgentThoughtChunk(ctx, entries, update.value, at);
          break;
      }

      return entries.slice();
    },
  };
}

/**
 * Project a whole page of frames at once — what `GetAcpReplayPage` hands back — into the entries to
 * prepend above the loaded range. `firstSeq` is the page's absolute start, and scopes the keys.
 */
export function projectReplayFrames(frames: AcpAgentMessage[], firstSeq: number): ChatMessage[] {
  const projector = createReplayProjector(firstSeq);
  let entries: ChatMessage[] = [];
  for (const frame of frames) entries = projector.append(frame);
  return entries;
}
