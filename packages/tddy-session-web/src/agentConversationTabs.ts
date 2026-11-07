/**
 * The tab-list algebra behind a session's agent conversation tabs.
 *
 * Pure, and returning new arrays: the list is React state. Keyed by `agentId` for membership and by
 * `conversationId` for identity, which is the distinction the tabs live on — an agent can be
 * attached with no conversation open, and closing a tab cancels a conversation, not an attachment.
 *
 * Feature: docs/ft/web/session-drawer.md § Add agent; invariants: packages/tddy-web/docs/session-agent-conversation.md.
 */

/** One open conversation with an agent attached to the session. */
export interface AgentConversation {
  /** The id the browser minted and the daemon was asked to open under. */
  readonly conversationId: string;
  /** The qualified `name@daemon_instance_id` of the agent being talked to. */
  readonly agentId: string;
  /** The agent's label as its host offered it. Empty when the daemon supplied none. */
  readonly label: string;
  /**
   * The daemon facilitating the session this conversation was opened on.
   *
   * Carried here rather than re-derived per render from the session list: it is known for certain at
   * attach time, and a conversation must be prompted and cancelled against the same daemon that
   * opened it. Re-deriving it means a session momentarily absent from the list yields `""`, which
   * does not mean "unknown" on the wire — it means "whichever daemon this request reached".
   */
  readonly daemonInstanceId: string;
}

/**
 * Open `conversation`, unless the session is already talking to that agent.
 *
 * A second `AttachSessionAgent` for an already-attached agent is a no-op on the roster
 * (docs/ft/daemon/session-agent-roster.md AC2), so growing a second tab for it would claim something
 * the daemon did not do.
 */
export function withAgentConversation(
  open: readonly AgentConversation[],
  conversation: AgentConversation,
): AgentConversation[] {
  if (conversationForAgent(open, conversation.agentId) !== null) return [...open];
  return [...open, conversation];
}

/** The conversation open with `agentId`, or `null` when the agent is attached but not being talked to. */
export function conversationForAgent(
  open: readonly AgentConversation[],
  agentId: string,
): AgentConversation | null {
  return open.find((c) => c.agentId === agentId) ?? null;
}

/**
 * What the tab is named. The host is dropped from an unlabelled agent's qualified id: it is on the
 * roster row, and a tab strip has no room for it.
 */
export function agentConversationLabel(conversation: AgentConversation): string {
  return conversation.label || conversation.agentId.split("@")[0];
}
