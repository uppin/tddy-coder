/**
 * Acceptance: the body of an agent conversation tab — the operator prompting an agent attached to
 * the session, and the agent's answer streaming back into the transcript.
 *
 * Feature: docs/ft/web/session-drawer.md § Add agent
 * Invariants: packages/tddy-web/docs/session-agent-conversation.md
 *
 * The answer is served by the in-memory backend as a *sequence* of `AgentConversationChunk` frames
 * rather than one stubbed value, because the property under test is what the pane does with a
 * stream: a stub could never distinguish "rendered the answer" from "folded three frames into one
 * turn". The daemon's framing contract is at `packages/tddy-service/proto/connection.proto:496-505`.
 *
 * Not to be confused with `SessionActivitiesPane.cy.tsx` / the Agent Activity overlay, which replay
 * a *session's* recorded ACP transcript. A roster agent has no such transcript (PRD § What is
 * deliberately not being built); this pane is a live conversation, not a replay.
 */

import React from "react";
import { SessionAgentConversationPane } from "../../src/components/sessions/SessionAgentConversationPane";
import { mountWithRpc } from "../support/rpc/inMemory";
import {
  anAgentConversationBackend,
  type AgentConversationBackend,
  type AgentConversationScenario,
} from "../support/rpc/agentConversationBackend";
import { sessionAgentConversationPage as page } from "../support/pages/sessionAgentConversationPage";

const SESSION_ID = "1780828020298-conversation";
const HOST = "workstation-1";
const EXPLORER = "explorer@workstation-1";
const CONVERSATION_ID = "conv-0198f0aa-0000-7000-8000-000000000001";

function mountPane(conversation: AgentConversationBackend) {
  mountWithRpc(
    <SessionAgentConversationPane
      sessionId={SESSION_ID}
      sessionToken="tok"
      daemonInstanceId={HOST}
      agentId={EXPLORER}
      conversationId={CONVERSATION_ID}
    />,
    conversation.backend,
  );
}

/** Mount the pane against an agent that answers with `scenario`'s frames. */
function anAgentThatAnswers(scenario: AgentConversationScenario): AgentConversationBackend {
  const conversation = anAgentConversationBackend(scenario);
  mountPane(conversation);
  return conversation;
}

describe("Agent conversation pane", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  // -------------------------------------------------------------------------
  // AC5 — the conversation the tab is keyed by
  // -------------------------------------------------------------------------

  it("opens the conversation under the id its tab is keyed by, naming the agent it is with", () => {
    // Given a daemon that will accept the open
    const conversation = anAgentConversationBackend({});

    // When the pane mounts
    mountPane(conversation);

    // Then the open names this session, this agent, and the caller-chosen id — a minted id would
    // leave the tab keyed by something the daemon never heard of
    cy.wrap(null).should(() => {
      expect(conversation.openedConversations()).to.deep.equal([
        {
          sessionId: SESSION_ID,
          daemonInstanceId: HOST,
          agentId: EXPLORER,
          conversationId: CONVERSATION_ID,
        },
      ]);
    });
  });

  it("names a conversation that could not be opened", () => {
    // Given an agent whose clone is not ready, so the daemon refuses the open
    anAgentThatAnswers({ openFails: "explorer@workstation-1 is still provisioning" });

    // Then the pane says so rather than offering an empty transcript
    page.error().should("contain.text", "explorer@workstation-1 is still provisioning");
  });

  // -------------------------------------------------------------------------
  // AC6 — the operator's prompt
  // -------------------------------------------------------------------------

  it("sends what the operator typed to the conversation the tab holds", () => {
    // Given an open conversation
    const conversation = anAgentThatAnswers({ answerChunks: ["ok"] });

    // When the operator asks something
    page.prompt("what does foo.rs do?");

    // Then it is sent under this conversation's id
    cy.wrap(null).should(() => {
      expect(conversation.promptsSent()).to.deep.equal([
        {
          sessionId: SESSION_ID,
          conversationId: CONVERSATION_ID,
          prompt: "what does foo.rs do?",
        },
      ]);
    });
  });

  it("shows the operator's prompt in the transcript as its own turn", () => {
    // Given an open conversation
    anAgentThatAnswers({ answerChunks: ["it parses the config file"] });

    // When the operator asks something
    page.prompt("what does foo.rs do?");

    // Then the prompt is the first turn, attributed to the operator
    page.assertTurn(0, "operator", "what does foo.rs do?");
  });

  // -------------------------------------------------------------------------
  // AC7 — the answer stream
  // -------------------------------------------------------------------------

  it("folds the answer's chunks into a single agent turn", () => {
    // Given an agent whose answer arrives in three frames
    anAgentThatAnswers({ answerChunks: ["it parses ", "the config ", "file"] });

    // When the operator asks
    page.prompt("what does foo.rs do?");

    // Then the transcript holds the prompt and one answer — not one turn per frame
    page.assertTurn(1, "agent", "it parses the config file");
    page.turns().should("have.length", 2);
  });

  it("closes the answer turn with the stop reason the final frame carries", () => {
    // Given an agent that ends its turn normally
    anAgentThatAnswers({ answerChunks: ["done"], stopReason: "EndTurn" });

    // When the operator asks
    page.prompt("finished?");

    // Then the turn is complete and says why it ended — a stream that ended without a final frame
    // was truncated, and the two must not look alike
    page.turn(1).should("have.attr", "data-complete", "true");
    page.turn(1).should("have.attr", "data-stop-reason", "EndTurn");
  });

  // -------------------------------------------------------------------------
  // AC8 — an empty answer is still an answer
  // -------------------------------------------------------------------------

  it("shows an empty answer as a completed agent turn rather than as no turn at all", () => {
    // Given an agent that has nothing to add — one frame, empty, marked last
    anAgentThatAnswers({ answerChunks: [""], stopReason: "EndTurn" });

    // When the operator asks
    page.prompt("anything to add?");

    // Then "said nothing" is visible as such, never as "nothing arrived"
    page.assertTurn(1, "agent", "");
    page.turn(1).should("have.attr", "data-complete", "true");
  });

  // -------------------------------------------------------------------------
  // AC9 — a failed prompt
  // -------------------------------------------------------------------------

  it("names a prompt that failed and keeps the turn that provoked it", () => {
    // Given a conversation whose prompt the daemon refuses
    anAgentThatAnswers({ promptFails: "the agent's loop is gone" });

    // When the operator asks
    page.prompt("still there?");

    // Then the failure is named ...
    page.error().should("contain.text", "the agent's loop is gone");
    // ... and the prompt stays on screen, so the operator can see what was lost
    page.assertTurn(0, "operator", "still there?");
  });

  // -------------------------------------------------------------------------
  // One turn at a time — a second prompt sent into an answer still arriving would interleave two
  // exchanges into one turn, because the projection extends whichever agent turn is still open.
  // -------------------------------------------------------------------------

  it("refuses a second prompt while the answer to the first is still arriving", () => {
    // Given a conversation whose answer has been asked for but is still on the wire
    const conversation = anAgentConversationBackend({
      answerChunks: ["still thinking"],
      holdAnswer: true,
    });
    mountPane(conversation);
    page.prompt("first question");
    page.assertTurn(0, "operator", "first question");

    // When the operator presses Enter on a second prompt before the first answer lands
    page.promptWithEnter("second question");

    // Then it was not sent. The Send button is disabled for exactly this reason, and Enter is the
    // other way in — a gate on the control alone would leave the keyboard as a way around it.
    cy.wrap(null).should(() => {
      expect(conversation.promptsSent().map((sent) => sent.prompt)).to.deep.equal([
        "first question",
      ]);
    });
  });

  it("sends the next prompt once the previous answer has ended", () => {
    // Given a conversation holding its first answer
    const conversation = anAgentConversationBackend({
      answerChunks: ["done"],
      holdAnswer: true,
    });
    mountPane(conversation);
    page.prompt("first question");

    // When the answer lands and the operator asks again
    cy.then(() => conversation.releaseAnswer());
    page.assertTurn(1, "agent", "done");
    page.prompt("second question");

    // Then the second prompt goes out — the gate is the turn in flight, not the conversation
    cy.wrap(null).should(() => {
      expect(conversation.promptsSent().map((sent) => sent.prompt)).to.deep.equal([
        "first question",
        "second question",
      ]);
    });
  });

  it("keeps the earlier exchange when a later prompt fails", () => {
    // Given a conversation that answered once and then lost its agent
    const conversation = anAgentConversationBackend({ answerChunks: ["yes"] });
    mountPane(conversation);
    page.prompt("are you there?");
    page.assertTurn(1, "agent", "yes");

    // When the next prompt fails
    cy.then(() => conversation.failNextPrompt("the agent's loop is gone"));
    page.prompt("and now?");

    // Then the transcript still holds the exchange that succeeded
    page.assertTurn(0, "operator", "are you there?");
    page.assertTurn(1, "agent", "yes");
    page.error().should("contain.text", "the agent's loop is gone");
  });
});
