/**
 * How an ACP tool call reads in a transcript, shared by the live chat and the read-only replay.
 *
 * Both surfaces receive the same two frames for one tool call — a `tool_call` announcing it and a
 * `tool_call_update` reporting how it ended — so the mapping from those frames onto a bubble's
 * status marker and text belongs in one place rather than in each surface's own switch.
 */

import { ToolCallStatus } from "../../gen/tddy/acp/v1/acp_pb";

/** The coarse status a transcript entry's marker shows. `awaiting_user` is a subagent bubble's alone:
 *  its turn is paused on a permission request the main agent handed to the user. */
export type TranscriptStatus = "running" | "completed" | "error" | "awaiting_user";

/**
 * Map an ACP `ToolCallStatus` onto the transcript's coarse status marker.
 *
 * Unspecified/pending/in-progress all read as "running": a call that has not said how it ended has
 * not ended. A missing status is the same claim, so it reads the same way — reporting it as
 * completed would tell the operator a tool finished on the strength of a field nobody set.
 */
export function toolStatusOf(status: ToolCallStatus | undefined): "running" | "completed" | "error" {
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
 * Map an ACP `ToolCallStatus` onto a subagent conversation's bubble. The session transcript folds a
 * conversation's `subagent_*` calls into one entry (docs/ft/web/agent-activity-pane.md § A subagent
 * conversation is one bubble). Its turn waiting on a permission request held for the user folds to
 * `pending`, and such a bubble reads as awaiting the user. A turn that paused on a request the main
 * agent answers itself is not awaiting the user, so it folds to in progress. Every other status reads
 * as it does for a tool call.
 */
export function subagentStatusOf(status: ToolCallStatus | undefined): TranscriptStatus {
  return status === ToolCallStatus.PENDING ? "awaiting_user" : toolStatusOf(status);
}

/**
 * The result a tool call produced, as text, from the `raw_output` its update carried.
 *
 * Tools answer in JSON, and the agent carries that answer through verbatim (`tool_output_value` in
 * `tddy-acp::provider_agent`): a JSON *string* is the tool's own prose, so it is unwrapped rather
 * than shown inside quotes, while any other JSON document is shown as it arrived. Text that is not
 * JSON at all is likewise shown as it arrived — the output is never dropped for failing to parse,
 * because a tool whose result cannot be displayed is indistinguishable from one that produced none.
 */
export function toolResultText(rawOutput: string | undefined): string {
  const raw = rawOutput?.trim() ?? "";
  if (raw === "") return "";
  try {
    const parsed: unknown = JSON.parse(raw);
    return typeof parsed === "string" ? parsed : raw;
  } catch {
    return raw;
  }
}

/**
 * The whole text of a tool call's bubble: what was called, and — once its update has arrived — what
 * it produced. A call with no result yet reads as just its title.
 */
export function toolEntryText(title: string, rawOutput: string | undefined): string {
  const result = toolResultText(rawOutput);
  return result === "" ? title : `${title}\n${result}`;
}
