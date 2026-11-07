/**
 * Acceptance: `PrStackChat`, mounted on its own through the package's entry point, streams the
 * agent's reply into the chat and shows a clarification question as an elicitation.
 *
 * The same behaviour is covered through the sessions drawer (`PrStackChatStreamingAcceptance`,
 * `PrStackChatElicitationAcceptance`); here the chat gets its presenter room from the media and
 * room-RPC port doubles, with no drawer in the tree.
 *
 * PRD: docs/ft/web/session-drawer.md § Per-Workflow Session Views (chat window).
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { AcpService } from "tddy-client-web/gen/tddy/acp/v1/acp_pb";
import { SessionEntrySchema } from "tddy-client-web/gen/session_pb";
import { PrStackChat } from "../../src/PrStackChat";
import { mountWithPresenterRoom, PRESENTER_IDENTITY } from "../support/mountWorkflowChat";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";
import { acpAgentChunk, acpQuestion, acpScriptedSession } from "../support/rpc/acpSession";

const PR_STACK_SESSION = create(SessionEntrySchema, {
  sessionId: "pr-stack-chat-direct-0000-0000-0000-000000000001",
  createdAt: "2026-07-03T09:00:00Z",
  status: "idle",
  repoPath: "/home/dev/pr-stack-project",
  projectId: "proj-pr-stack-chat-direct",
  recipe: "pr-stack",
});

const SENTENCE = "I planned three PRs for the auth work.";

/** Mount the chat over an ACP agent that plays `session`. */
function mountChatOver(session: ReturnType<typeof acpScriptedSession>) {
  mountWithPresenterRoom(
    (room) => <PrStackChat session={PR_STACK_SESSION} room={room} livekitServerIdentity={PRESENTER_IDENTITY} roomStatus="connected" />,
    anInMemoryRpcBackend().implement(AcpService, { session }),
  );
}

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("streams the agent's reply into one chat bubble", () => {
  // Given — an agent that streams its sentence token by token
  const tokens = ["I planned", " three PRs", " for the", " auth work."];

  // When
  mountChatOver(acpScriptedSession(...tokens.map(acpAgentChunk), acpAgentChunk("\n")));

  // Then
  prStackScreenPage.chatMessage(0).should("have.text", SENTENCE);
  prStackScreenPage.chatMessage(1).should("not.exist");
});

it("shows the agent's clarification question with its options", () => {
  // Given — an agent that pauses on a question
  const question = acpQuestion(["Claude", "Cursor"], {
    multi: false,
    allowOther: true,
    header: "Backend",
    question: "Which coding backend should drive this PR stack?",
  });

  // When
  mountChatOver(acpScriptedSession(question));

  // Then
  prStackScreenPage.chatQuestionHeader().should("contain.text", "Backend");
  prStackScreenPage.chatQuestionText().should("contain.text", "Which coding backend should drive this PR stack?");
  prStackScreenPage.chatOption(0).should("contain.text", "Claude");
  prStackScreenPage.chatOption(1).should("contain.text", "Cursor");
});
