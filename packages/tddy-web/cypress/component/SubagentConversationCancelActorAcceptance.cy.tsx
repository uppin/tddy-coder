/**
 * Acceptance: closing a conversation cancels it without naming a login, and a close with no
 * signed-in user is refused rather than sent.
 *
 * The daemon attributes a cancel to the authenticated caller itself, so the web sends no actor. A
 * permission request still pending on the conversation is recorded as answered by whoever cancels
 * it. The web's one check is that someone is signed in: with no signed-in user there is nothing to
 * cancel as, so the cancel is refused and not sent.
 *
 * Feature: docs/ft/web/session-drawer.md § Add agent
 * Invariants: packages/tddy-web/docs/session-agent-conversation.md
 */

import React, { useState } from "react";
import {
  SessionAgentConversationPane,
} from "../../src/components/sessions/SessionAgentConversationPane";
import { SessionAgentService } from "../../src/gen/session_agents_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { recordedFields } from "../support/rpc/recordedRequests";
import {
  anAgentConversationBackend,
  type AgentConversationBackend,
} from "../support/rpc/agentConversationBackend";
import { byTestId } from "../support/testIds";

const SESSION_ID = "1780828020298-cancel-actor";
const HOST = "workstation-1";
const EXPLORER = "explorer@workstation-1";
const CONVERSATION_ID = "conv-0198f0aa-0000-7000-8000-0000000000c1";
const SESSION_TOKEN = "tok";

/**
 * The pane with a button that closes it. Closing a conversation tab unmounts its body, and the
 * unmount is what cancels the conversation, so the test closes it the same way.
 */
function PaneThatCanBeClosed({ callerLogin }: { callerLogin: string | null }) {
  const [open, setOpen] = useState(true);
  return (
    <>
      {open ? (
        <SessionAgentConversationPane
          sessionId={SESSION_ID}
          sessionToken={SESSION_TOKEN}
          daemonInstanceId={HOST}
          agentId={EXPLORER}
          conversationId={CONVERSATION_ID}
          callerLogin={callerLogin}
        />
      ) : null}
      <button type="button" data-testid="close-the-pane" onClick={() => setOpen(false)}>
        close
      </button>
    </>
  );
}

/** Open the pane for `callerLogin` and wait until the daemon has the conversation to cancel. */
function openAConversationFor(callerLogin: string | null): AgentConversationBackend {
  const conversation = anAgentConversationBackend({});
  mountWithRpc(<PaneThatCanBeClosed callerLogin={callerLogin} />, conversation.backend);
  cy.wrap(null).should(() => {
    expect(conversation.openedConversations()).to.have.length(1);
  });
  return conversation;
}

describe("Agent conversation cancel — the daemon attributes it, the web names no login", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  it("sends the cancel for a signed-in user without naming a login", () => {
    // Given a conversation the operator opened while signed in
    const conversation = openAConversationFor("alice");

    // When the operator closes the tab
    byTestId("close-the-pane").click();

    // Then the cancel is sent for the conversation, and names no login: the daemon attributes it to
    // the authenticated caller, so an actor sent from here would only be a second, unchecked one
    cy.wrap(null).should(() => {
      const sent = recordedFields(
        conversation.backend.callsTo(SessionAgentService.method.cancelAgentConversation),
      );
      expect(sent).to.have.length(1);
      expect(sent[0]).to.deep.equal({
        sessionToken: SESSION_TOKEN,
        sessionId: SESSION_ID,
        daemonInstanceId: HOST,
        conversationId: CONVERSATION_ID,
      });
    });
  });

  it("refuses the cancel and sends nothing when no user is signed in", () => {
    // Given a conversation opened with no signed-in user
    const conversation = openAConversationFor(null);

    // When the operator closes the tab
    byTestId("close-the-pane").click();

    // Then the tab is gone, and nothing was cancelled: with no signed-in user there is no caller
    // for the daemon to attribute the cancel to, so the refusal is the only outcome
    byTestId("agent-conversation-pane").should("not.exist");
    cy.wrap(null).should(() => {
      expect(
        conversation.backend.callsTo(SessionAgentService.method.cancelAgentConversation),
      ).to.have.length(0);
    });
  });
});
