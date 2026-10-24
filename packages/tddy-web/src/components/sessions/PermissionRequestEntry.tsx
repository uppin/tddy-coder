import type { Client } from "@connectrpc/connect";
import type { SessionAgentService } from "../../gen/session_agents_pb";

/** A coding subagent's pending permission request, as the conversation transcript records it. */
export interface PendingPermission {
  requestId: string;
  title: string;
  options: { optionId: string; name: string; kind: string }[];
  /** The main agent handed it to the user — only then does the entry offer the options. */
  deferredToUser: boolean;
  /** Who answered it, once answered. */
  answeredBy?: string;
}

export interface PermissionRequestEntryProps {
  sessionId: string;
  conversationId: string;
  sessionToken: string;
  permission: PendingPermission;
  client: Client<typeof SessionAgentService>;
}

/**
 * The request's tool and, when the main agent deferred it to the user, one button per option; a
 * choice is sent with `AnswerAgentPermission` and the entry then records it and who made it.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-web-approvals.md
 */
export function PermissionRequestEntry(_props: PermissionRequestEntryProps) {
  // TODO(web-approvals): implement
  return null;
}
