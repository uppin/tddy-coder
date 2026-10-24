import type { PendingPermission } from "./PermissionRequestEntry";

/**
 * A subagent conversation's permission request, as its transcript records it: a tool call whose id is
 * `permission:<requestId>`, whose input carries the request — its title, its options and whether the
 * main agent deferred it to the user.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-web-approvals.md
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
