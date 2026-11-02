import type React from "react";
import type { SessionEntry } from "../../gen/session_pb";
import type { AgentConversation } from "./agentConversationTabs";
import type { SessionAgentClient, TerminalClient } from "./sessionRuntimeProps";

/**
 * The tab strip of a session whose agent is not running.
 *
 * A live session's strip lives in `SessionRuntime`, which needs a connection to its agent. A stopped
 * session has none, yet its worktree is still there: "+" opens bash and harness terminals through the
 * owning daemon's `TerminalSessionService` (which starts them in the worktree the session's record
 * names), the terminals the daemon already runs for it are listed, and added agents' conversations
 * get their tabs. What the Agent tab shows is the caller's — `agentTab` — so the stopped Agent tab's
 * body can change without touching the strip.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-02-dormant-session-tabs.md (`#session-tabs 1/7`)
 */
export interface DormantSessionRuntimeProps {
  /** The stopped session. */
  session: SessionEntry;
  sessionToken: string;
  /** The signed-in user's login — an agent-conversation tab needs it to cancel when closed. */
  callerLogin?: string | null;
  /** Owning daemon `TerminalSessionService` client — the terminals' only wire. */
  terminalClient?: TerminalClient | null;
  /** Owning daemon `SessionAgentService` client — the conversation panes hold their turns over it. */
  client?: SessionAgentClient | null;
  /** The body of the Agent tab. */
  agentTab: React.ReactNode;
  /** Open conversations with agents attached to this session, one tab and one pane each. */
  agentConversations?: readonly AgentConversation[];
  /** The focused conversation's id, or `null` when a terminal tab holds the pane. */
  activeAgentConversationId?: string | null;
  /** Focus a conversation, or `null` to hand the pane back to the terminal tabs. */
  onSelectAgentConversation?: (conversationId: string | null) => void;
  /** Close a conversation's tab — the owner drops it, which cancels it. */
  onCloseAgentConversation?: (conversationId: string) => void;
}

export function DormantSessionRuntime(_props: DormantSessionRuntimeProps): React.ReactElement | null {
  // TODO(dormant-tabs): implement
  return null;
}
