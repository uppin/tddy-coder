import type { PendingPermission } from "./PermissionRequestEntry";

/**
 * A subagent conversation's permission request, as its transcript records it: a tool call whose id is
 * `permission:<requestId>`, whose input carries the request — its title, its options and whether the
 * main agent deferred it to the user.
 *
 * Feature: docs/ft/web/agent-activity-pane.md § The user answers a deferred permission request
 */

/** The `tool_call_id` prefix of a transcript entry that records a permission request. */
export const PERMISSION_TOOL_CALL_PREFIX = "permission:";

/** The request a `tool_call_id` names, when it names one; else `null`. */
export function permissionRequestIdOf(toolCallId: string): string | null {
  if (!toolCallId.startsWith(PERMISSION_TOOL_CALL_PREFIX)) return null;
  const requestId = toolCallId.slice(PERMISSION_TOOL_CALL_PREFIX.length);
  return requestId === "" ? null : requestId;
}

/**
 * The request a tool call's raw input carries, or `null` when the input is not one. A `null` is shown
 * to the operator as an unreadable request; nothing is guessed from a partial one.
 */
export function permissionRequestOf(rawInput: string | undefined): PendingPermission | null {
  if (rawInput === undefined) return null;
  let parsed: unknown;
  try {
    parsed = JSON.parse(rawInput);
  } catch {
    return null;
  }
  if (typeof parsed !== "object" || parsed === null) return null;
  const { requestId, title, options, deferredToUser } = parsed as Record<string, unknown>;
  if (
    typeof requestId !== "string" ||
    typeof title !== "string" ||
    typeof deferredToUser !== "boolean" ||
    !Array.isArray(options) ||
    !options.every(isOption)
  ) {
    return null;
  }
  return { requestId, title, options, deferredToUser };
}

function isOption(value: unknown): value is PendingPermission["options"][number] {
  if (typeof value !== "object" || value === null) return false;
  const { optionId, name, kind } = value as Record<string, unknown>;
  return typeof optionId === "string" && typeof name === "string" && typeof kind === "string";
}

/** Who answered a permission request, and with which option, as its answered frame records it. */
export interface AnsweredRecord {
  readonly answeredBy: string;
  /** The option the answer chose; `null` when the answer was a cancellation. */
  readonly optionId: string | null;
}

/**
 * The answer a permission request's transcript records: the raw output of its tool call once that
 * call completed, `{answeredBy, optionId}` for a chosen option or `{answeredBy, cancelled: true}`
 * for a cancellation. Any other output is not an answer, so the request reads as still unanswered.
 */
export function answeredRecordOf(rawOutput: string | undefined): AnsweredRecord | null {
  if (rawOutput === undefined) return null;
  let parsed: unknown;
  try {
    parsed = JSON.parse(rawOutput);
  } catch {
    return null;
  }
  if (typeof parsed !== "object" || parsed === null) return null;
  const { answeredBy, optionId, cancelled } = parsed as Record<string, unknown>;
  if (typeof answeredBy !== "string" || answeredBy.trim() === "") return null;
  if (cancelled === true) return { answeredBy, optionId: null };
  if (typeof optionId === "string" && optionId !== "") return { answeredBy, optionId };
  return null;
}

/** The answer fields an entry shows for a permission request, read from its recorded answer. */
export interface PermissionAnswerFields {
  /** Who answered the request, or who cancelled it. */
  readonly answeredBy: string;
  /** True when the request was cancelled rather than answered with an option. */
  readonly answeredCancelled: boolean;
  /** The option the answer chose; absent for a cancellation. */
  readonly answeredOptionId?: string;
}

/**
 * The answer fields a permission request's completed tool call records, or `null` when it records
 * no answer. A cancellation names the actor who cancelled the request and carries no option; an
 * option answer names the option and the user who chose it.
 */
export function answerFieldsOf(rawOutput: string | undefined): PermissionAnswerFields | null {
  const record = answeredRecordOf(rawOutput);
  if (record === null) return null;
  if (record.optionId === null) {
    return { answeredBy: record.answeredBy, answeredCancelled: true };
  }
  return {
    answeredBy: record.answeredBy,
    answeredCancelled: false,
    answeredOptionId: record.optionId,
  };
}
