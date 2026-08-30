import { describe, expect, it } from "bun:test";
import {
  agentConversationLabel,
  conversationForAgent,
  withAgentConversation,
  type AgentConversation,
} from "./agentConversationTabs";

/**
 * The pure tab-list algebra behind a session's agent conversation tabs
 * (docs/ft/web/session-drawer.md § Add agent).
 *
 * The rule worth pinning here is that attaching an agent twice must not open a second tab:
 * `AttachSessionAgent` is a no-op on the roster the second time (session-agent-roster.md AC2), so a
 * UI that grew a tab per click would be claiming something the daemon did not do.
 */

const EXPLORER = "explorer@workstation-1";
const LINTER = "linter@server-2";

function aConversation(
  agentId: string,
  conversationId: string,
  label = "",
): AgentConversation {
  // The host is part of the conversation's identity, not decoration: it is what the prompt and the
  // cancel are routed to, so a builder that omitted it could not express a wrong one.
  return { agentId, conversationId, label, daemonInstanceId: agentId.split("@")[1] ?? "" };
}

describe("agentConversationTabs", () => {
  describe("withAgentConversation", () => {
    it("adds the first conversation to an empty list", () => {
      // Given no open conversations
      // When one is opened
      const open = withAgentConversation([], aConversation(EXPLORER, "conv-1"));

      // Then
      expect(open).toEqual([aConversation(EXPLORER, "conv-1")]);
    });

    it("keeps one tab per agent when the same agent is attached again", () => {
      // Given a conversation already open with the explorer
      const open = withAgentConversation([], aConversation(EXPLORER, "conv-1"));

      // When the operator attaches the explorer a second time
      const reopened = withAgentConversation(open, aConversation(EXPLORER, "conv-2"));

      // Then the original conversation stands — a second attach changed nothing on the roster, so
      // it must not look like it opened a second conversation here
      expect(reopened).toEqual([aConversation(EXPLORER, "conv-1")]);
    });

    it("appends a conversation with a different agent after the existing ones", () => {
      // Given the explorer's conversation is open
      const open = withAgentConversation([], aConversation(EXPLORER, "conv-1"));

      // When the linter is attached too
      const both = withAgentConversation(open, aConversation(LINTER, "conv-2"));

      // Then both tabs stand, in the order they were opened
      expect(both.map((c) => c.agentId)).toEqual([EXPLORER, LINTER]);
    });

    it("leaves the list it was given untouched", () => {
      // Given an open conversation
      const open = withAgentConversation([], aConversation(EXPLORER, "conv-1"));

      // When another agent is added
      withAgentConversation(open, aConversation(LINTER, "conv-2"));

      // Then the input list is unchanged — it is React state
      expect(open).toEqual([aConversation(EXPLORER, "conv-1")]);
    });
  });

  describe("conversationForAgent", () => {
    it("finds the conversation open with an agent", () => {
      // Given two open conversations
      const open = [aConversation(EXPLORER, "conv-1"), aConversation(LINTER, "conv-2")];

      // When the linter's is looked up
      const found = conversationForAgent(open, LINTER);

      // Then
      expect(found).toEqual(aConversation(LINTER, "conv-2"));
    });

    it("returns null for an agent with no conversation open", () => {
      // Given only the explorer is being talked to
      const open = [aConversation(EXPLORER, "conv-1")];

      // When an agent that is attached but silent is looked up
      const found = conversationForAgent(open, LINTER);

      // Then — attached and "has a tab open" are different facts
      expect(found).toBeNull();
    });
  });

  describe("agentConversationLabel", () => {
    it("uses the def's label when the daemon supplied one", () => {
      // Given a conversation with a labelled agent
      const conversation = aConversation(EXPLORER, "conv-1", "Explorer");

      // When
      const label = agentConversationLabel(conversation);

      // Then
      expect(label).toEqual("Explorer");
    });

    it("falls back to the bare name of the qualified id when there is no label", () => {
      // Given an unlabelled agent whose id names its host
      const conversation = aConversation(EXPLORER, "conv-1");

      // When
      const label = agentConversationLabel(conversation);

      // Then the host is dropped: it is on the roster row, and a tab strip has no room for it
      expect(label).toEqual("explorer");
    });
  });
});
