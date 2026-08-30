/**
 * The transcript behind an agent conversation tab: `AgentConversationChunk` frames folded into
 * turns.
 *
 * The daemon frames an answer as one or more chunks, only the last of which is marked `last` and
 * carries a `stop_reason`, and it guarantees **exactly one** frame even for an empty answer
 * (`packages/tddy-service/proto/connection.proto:496-505`) — so "the agent said nothing" is a turn
 * here, never an absence.
 *
 * Pure, and returning new arrays: the turns are React state, and folding a chunk into the array in
 * place would update the transcript without ever re-rendering the tab.
 *
 * Feature: docs/ft/web/session-drawer.md § Add agent; invariants: packages/tddy-web/docs/session-agent-conversation.md.
 */

/** One side of one exchange. An operator turn is complete the moment it is sent. */
export interface AgentTurn {
  readonly role: "operator" | "agent";
  readonly text: string;
  /** Why the agent's turn ended ("EndTurn", "MaxTurns", …). Empty until the final frame arrives. */
  readonly stopReason: string;
  readonly complete: boolean;
}

/**
 * One frame of an answer, as `PromptAgentConversation` yields it. Structural rather than the
 * generated `AgentConversationChunk` so the projection can be exercised without the wire types.
 */
export interface AgentAnswerChunk {
  readonly contentChunk: string;
  readonly stopReason: string;
  readonly last: boolean;
}

/** Record the prompt the operator just sent. It leaves complete — nothing more is added to it. */
export function appendOperatorTurn(
  turns: readonly AgentTurn[],
  prompt: string,
): AgentTurn[] {
  return [...turns, { role: "operator", text: prompt, stopReason: "", complete: true }];
}

/**
 * Fold one answer frame into the transcript: it extends the agent turn still being spoken, or opens
 * a new one when the last turn is the operator's or an answer that has already ended.
 */
export function appendAnswerChunk(
  turns: readonly AgentTurn[],
  chunk: AgentAnswerChunk,
): AgentTurn[] {
  const open = turns[turns.length - 1];
  const extending = open !== undefined && open.role === "agent" && !open.complete;
  const turn: AgentTurn = {
    role: "agent",
    text: (extending ? open.text : "") + chunk.contentChunk,
    stopReason: chunk.stopReason,
    complete: chunk.last,
  };
  return extending ? [...turns.slice(0, -1), turn] : [...turns, turn];
}
