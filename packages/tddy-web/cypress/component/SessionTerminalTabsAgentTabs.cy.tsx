/**
 * Acceptance: the session tab strip's fourth tab kind — a conversation with an agent attached to
 * the session, alongside the fixed Agent tab, the bash tabs and the spawned child-conversation tabs.
 *
 * Feature: docs/ft/web/session-drawer.md § Add agent
 * Invariants: packages/tddy-web/docs/session-agent-conversation.md
 *
 * The strip is mounted on its own, without RPC: what it does with the conversations it is handed is
 * a rendering question, and driving it through a whole attached session would only make a failure
 * harder to place. `SessionAgentAttachTabAcceptance.cy.tsx` covers the wiring that fills it.
 *
 * Keyed by *conversation* id rather than agent id throughout, because that is what the tab owns: an
 * agent can be attached with no conversation open, and closing a tab cancels a conversation, not an
 * attachment.
 */

import React from "react";
import { SessionTerminalTabs } from "../../src/components/sessions/SessionTerminalTabs";
import type { AgentConversation } from "../../src/components/sessions/agentConversationTabs";
import { AGENT_TERMINAL_ID } from "../../src/components/sessions/useSessionTerminals";
import { sessionTerminalTabsPage as tabs } from "../support/pages/sessionTerminalTabsPage";

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

const EXPLORER_CONVERSATION = "conv-0198f0aa-0000-7000-8000-000000000001";
const LINTER_CONVERSATION = "conv-0198f0aa-0000-7000-8000-000000000002";

function aConversation(
  agentId: string,
  conversationId: string,
  label = "",
): AgentConversation {
  return { agentId, conversationId, label, daemonInstanceId: agentId.split("@")[1] ?? "" };
}

interface StripOptions {
  /** The focused conversation, or null when a terminal tab holds focus. */
  activeAgentConversationId?: string | null;
}

/** The tab strip carrying `conversations`, with the Agent terminal focused unless told otherwise. */
function aTabStrip(conversations: AgentConversation[], options: StripOptions = {}) {
  const onSelectAgentConversation = cy.stub().as("onSelectAgentConversation");
  const onCloseAgentConversation = cy.stub().as("onCloseAgentConversation");
  const driver = {
    mount() {
      cy.mount(
        <SessionTerminalTabs
          terminals={[]}
          activeTerminalId={AGENT_TERMINAL_ID}
          onSelect={cy.stub().as("onSelect")}
          onOpen={cy.stub().as("onOpen")}
          onClose={cy.stub().as("onClose")}
          agentConversations={conversations}
          activeAgentConversationId={options.activeAgentConversationId ?? null}
          onSelectAgentConversation={onSelectAgentConversation}
          onCloseAgentConversation={onCloseAgentConversation}
        />,
      );
      return driver;
    },
  };
  return driver;
}

// ---------------------------------------------------------------------------

describe("SessionTerminalTabs — conversations with attached agents", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  it("renders no agent tabs when no conversation is open", () => {
    // Given a session nobody has talked to an agent in
    // When the strip renders
    aTabStrip([]).mount();

    // Then only the Agent terminal tab is there
    tabs.agentTab().should("exist");
    tabs.agentConversationTabs().should("not.exist");
  });

  it("renders one tab per open conversation, labelled by the agent it is with", () => {
    // Given conversations with two different agents
    aTabStrip([
      aConversation("explorer@workstation-1", EXPLORER_CONVERSATION, "Explorer"),
      aConversation("linter@server-2", LINTER_CONVERSATION, "Linter"),
    ]).mount();

    // Then each has its own tab, named after its agent
    tabs.agentConversationTab(EXPLORER_CONVERSATION).should("have.text", "Explorer");
    tabs.agentConversationTab(LINTER_CONVERSATION).should("have.text", "Linter");
  });

  it("names an unlabelled agent by the bare name of its qualified id", () => {
    // Given an agent the daemon supplied no label for
    aTabStrip([aConversation("explorer@workstation-1", EXPLORER_CONVERSATION)]).mount();

    // Then the host is dropped — it lives on the roster row, and a tab strip has no room for it
    tabs.agentConversationTab(EXPLORER_CONVERSATION).should("have.text", "explorer");
  });

  it("marks the focused conversation's tab selected and the Agent tab not", () => {
    // Given a conversation that holds focus
    aTabStrip([aConversation("explorer@workstation-1", EXPLORER_CONVERSATION, "Explorer")], {
      activeAgentConversationId: EXPLORER_CONVERSATION,
    }).mount();

    // Then the strip says so on both tabs — a terminal tab left selected beside it would claim two
    // panes are showing at once
    tabs
      .agentConversationTab(EXPLORER_CONVERSATION)
      .should("have.attr", "aria-selected", "true");
    tabs.agentTab().should("have.attr", "aria-selected", "false");
  });

  it("leaves every agent tab unselected while a terminal holds focus", () => {
    // Given open conversations, with the Agent terminal focused
    aTabStrip([
      aConversation("explorer@workstation-1", EXPLORER_CONVERSATION, "Explorer"),
      aConversation("linter@server-2", LINTER_CONVERSATION, "Linter"),
    ]).mount();

    // Then
    tabs
      .agentConversationTab(EXPLORER_CONVERSATION)
      .should("have.attr", "aria-selected", "false");
    tabs.agentTab().should("have.attr", "aria-selected", "true");
  });

  it("asks for the conversation its tab names when the tab is clicked", () => {
    // Given two open conversations
    aTabStrip([
      aConversation("explorer@workstation-1", EXPLORER_CONVERSATION, "Explorer"),
      aConversation("linter@server-2", LINTER_CONVERSATION, "Linter"),
    ]).mount();

    // When the second one's tab is clicked
    tabs.agentConversationTab(LINTER_CONVERSATION).click();

    // Then it is the one asked for
    cy.get("@onSelectAgentConversation").should("have.been.calledOnceWith", LINTER_CONVERSATION);
  });

  it("asks to close the conversation its tab names when the close control is clicked", () => {
    // Given an open conversation
    aTabStrip([aConversation("explorer@workstation-1", EXPLORER_CONVERSATION, "Explorer")], {
      activeAgentConversationId: EXPLORER_CONVERSATION,
    }).mount();

    // When its ✕ is clicked
    tabs.agentConversationTabClose(EXPLORER_CONVERSATION).click();

    // Then the close names the conversation, not the agent — cancelling is per conversation
    cy.get("@onCloseAgentConversation").should("have.been.calledOnceWith", EXPLORER_CONVERSATION);
  });

  it("does not select a conversation when its close control is clicked", () => {
    // Given a conversation that does not hold focus
    aTabStrip([aConversation("explorer@workstation-1", EXPLORER_CONVERSATION, "Explorer")]).mount();

    // When its ✕ is clicked
    tabs.agentConversationTabClose(EXPLORER_CONVERSATION).click();

    // Then closing it is not also a request to look at it
    cy.get("@onSelectAgentConversation").should("not.have.been.called");
  });
});
