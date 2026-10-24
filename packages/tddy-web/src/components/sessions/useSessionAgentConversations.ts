import React from "react";
import { ConnectError } from "@connectrpc/connect";
import type { SessionEntry } from "../../gen/session_pb";
import type { AvailableAgent } from "./useAvailableAgents";
import { agentPlacementWire, type AgentPlacementChoice } from "./agentPlacementChoice";
import {
  conversationForAgent,
  withAgentConversation,
  type AgentConversation,
} from "./agentConversationTabs";
import { randomUuid } from "../../lib/randomId";
import type { SessionAgentClient } from "./sessionMainPaneProps";

/**
 * The specialized agents attached to each session, and which of their conversations holds that
 * session's pane — lifted out of `SessionMainPane` unchanged, with the Add-agent picker's open flag.
 */
export function useSessionAgentConversations({
  sessionAgentClient,
  selectedSession,
  sessionToken,
}: {
  sessionAgentClient?: SessionAgentClient;
  selectedSession: SessionEntry | null;
  sessionToken: string;
}) {
  // The specialized agents attached to each session, and which of their conversations holds that
  // session's pane. Held here rather than inside `SessionRuntime` because a runtime is backgrounded
  // (not unmounted) on a session switch and must come back with its conversations intact — only
  // closing a tab ends one.
  const [agentConversations, setAgentConversations] = React.useState<
    Record<string, AgentConversation[]>
  >({});
  const [activeConversations, setActiveConversations] = React.useState<
    Record<string, string | null>
  >({});
  const [pickerOpen, setPickerOpen] = React.useState(false);

  const focusConversation = React.useCallback(
    (sessionId: string, conversationId: string | null) => {
      setActiveConversations((current) => ({ ...current, [sessionId]: conversationId }));
    },
    [],
  );

  /**
   * Attach the picked agent to the session on screen and open a conversation tab with it.
   *
   * The conversation id is minted here, by the browser, so the tab is keyed by an id it can also
   * cancel — `OpenAgentConversation` accepts a caller-chosen one precisely for that. It is minted
   * with `randomUuid`, never `crypto.randomUUID`, which is undefined on the plain-http LAN origins
   * this app is routinely served from. The `OpenAgentConversation` call itself belongs to the tab's
   * body, which owns the conversation for as long as the tab is open.
   */
  const attachAgent = async (
    agent: AvailableAgent,
    placement: AgentPlacementChoice,
  ): Promise<string | null> => {
    // The picker is only rendered with both in hand; a refusal names what is missing rather than
    // reporting an attach that was never sent as a success.
    if (!sessionAgentClient || !selectedSession) {
      return "Not connected to this session's daemon.";
    }
    const { sessionId, daemonInstanceId } = selectedSession;
    try {
      await sessionAgentClient.attachSessionAgent({
        sessionToken,
        sessionId,
        daemonInstanceId,
        agentId: agent.agentId,
        placement: agentPlacementWire(placement),
      });
    } catch (err) {
      return ConnectError.from(err).rawMessage;
    }

    // Attaching an agent that is already attached is a no-op on the roster, so a second tab would
    // claim something the daemon did not do: the conversation already open is focused instead.
    const open = agentConversations[sessionId] ?? [];
    const existing = conversationForAgent(open, agent.agentId);
    const conversation: AgentConversation = existing ?? {
      conversationId: randomUuid(),
      agentId: agent.agentId,
      label: agent.label,
      daemonInstanceId,
    };
    if (existing === null) {
      setAgentConversations((current) => ({
        ...current,
        [sessionId]: withAgentConversation(current[sessionId] ?? [], conversation),
      }));
    }
    focusConversation(sessionId, conversation.conversationId);
    return null;
  };

  /** Drop a conversation's tab. Unmounting its body is what cancels the conversation. */
  const closeConversation = React.useCallback(
    (sessionId: string, conversationId: string) => {
      setAgentConversations((current) => ({
        ...current,
        [sessionId]: (current[sessionId] ?? []).filter(
          (c) => c.conversationId !== conversationId,
        ),
      }));
      setActiveConversations((current) => ({
        ...current,
        [sessionId]: current[sessionId] === conversationId ? null : (current[sessionId] ?? null),
      }));
    },
    [],
  );

  return {
    agentConversations,
    activeConversations,
    pickerOpen,
    setPickerOpen,
    focusConversation,
    attachAgent,
    closeConversation,
  };
}
