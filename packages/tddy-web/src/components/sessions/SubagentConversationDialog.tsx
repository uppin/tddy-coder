import type { Client } from "@connectrpc/connect";
import type { ActivityService } from "../../gen/activity_pb";

export interface SubagentConversationDialogProps {
  sessionId: string;
  /** The conversation the clicked bubble stands for. */
  conversationId: string;
  /** The bubble's text — the agent and what it was asked — shown as the dialog's title. */
  title: string;
  sessionToken: string;
  client: Client<typeof ActivityService>;
  onClose: () => void;
}

/**
 * A subagent conversation itself, opened from its activity bubble: the conversation's own ACP
 * transcript in the activity pane's read-only view (`AgentChatView readOnly` over `useAcpReplay`
 * with the conversation's id), live while it runs, its tool calls opening `AgentActivityDetailDialog`
 * like the pane's. A conversation that recorded nothing (an in-jail loop) says so.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-conversation-bubble.md
 */
export function SubagentConversationDialog(_props: SubagentConversationDialogProps) {
  // TODO(conversation-bubble): implement
  return null;
}
