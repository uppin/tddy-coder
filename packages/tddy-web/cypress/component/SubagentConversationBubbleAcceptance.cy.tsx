/**
 * Acceptance: a subagent conversation is ONE bubble in the session's activity pane, and clicking it
 * opens the conversation itself — the subagent's own transcript, live, its tool calls opening their
 * detail as in the pane.
 *
 * Feature: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-conversation-bubble.md
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { createClient } from "@connectrpc/connect";
import { ActivityService } from "../../src/gen/activity_pb";
import { SessionEntrySchema } from "../../src/gen/session_pb";
import { SessionWithActivitySidebar } from "../../src/components/sessions/SessionWithActivitySidebar";
import { mountWithRpc } from "../support/rpc/inMemory";
import { agentActivityPage } from "../support/pages/agentActivityPage";
import { byTestId } from "../support/testIds";
import {
  ToolCallStatus,
  ToolKind,
  aConversationReplayBackend,
  aToolDetail,
  replayAgentText,
  replayToolCall,
} from "../support/rpc/acpReplay";

const CONVERSATION = "conv-1";

const THE_SUBAGENT_BUBBLE = replayToolCall({
  id: `subagent:${CONVERSATION}`,
  title: "codex-fast · Find the parser",
  kind: ToolKind.OTHER,
  status: ToolCallStatus.IN_PROGRESS,
  atUnixMs: 1_000,
});

function aManagedSession() {
  return create(SessionEntrySchema, {
    sessionId: "managed-1",
    isActive: true,
    status: "active",
    sessionType: "claude-cli",
    managedCodebase: true,
  });
}

function mountTheSession(backend: ReturnType<typeof aConversationReplayBackend>["backend"]) {
  const client = createClient(ActivityService, backend.transport());
  mountWithRpc(
    <div style={{ display: "flex", flexDirection: "column", height: 700, width: 1200 }}>
      <SessionWithActivitySidebar
        session={aManagedSession()}
        sessionToken="tok"
        client={client}
        suppressed={false}
        toolbar={(toggle) => <div data-testid="session-toolbar">{toggle}</div>}
      >
        <div data-testid="session-main-content">terminal</div>
      </SessionWithActivitySidebar>
    </div>,
    backend,
  );
}

/** A session whose activity holds one subagent conversation, recorded as `conversation`. */
function aSessionWithOneSubagentConversation(
  conversation: Parameters<typeof aConversationReplayBackend>[0]["conversations"][string],
) {
  return aConversationReplayBackend({
    session: { counts: [1], snapshot: [THE_SUBAGENT_BUBBLE] },
    conversations: { [CONVERSATION]: conversation },
  });
}

const A_RECORDED_CONVERSATION = {
  counts: [2],
  snapshot: [
    replayAgentText("The parser is in src/parser.rs.", 2_000),
    replayToolCall({
      id: "sub-read-1",
      title: "Read src/parser.rs",
      kind: ToolKind.READ,
      status: ToolCallStatus.COMPLETED,
      atUnixMs: 2_500,
    }),
  ],
  details: { "sub-read-1": aToolDetail({ input: { file_path: "src/parser.rs" }, output: "fn parse() {}" }) },
};

function theSubagentBubble() {
  return byTestId("agent-chat-messages").find('[data-message-kind="subagent"]');
}

function theConversationDialog() {
  return byTestId("subagent-conversation-dialog", { timeout: 5000 });
}

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("one subagent conversation renders as one bubble with its agent label and status", () => {
  // Given
  const { backend } = aSessionWithOneSubagentConversation(A_RECORDED_CONVERSATION);

  // When
  mountTheSession(backend);

  // Then
  agentActivityPage.sidebar().should("be.visible");
  theSubagentBubble().should("have.length", 1).and("contain.text", "codex-fast · Find the parser");
  byTestId("agent-chat-messages")
    .find('[data-message-kind="tool"]')
    .should("not.exist");
});

it("clicking the bubble opens the conversation transcript", () => {
  // Given
  const { backend } = aSessionWithOneSubagentConversation(A_RECORDED_CONVERSATION);
  mountTheSession(backend);

  // When
  theSubagentBubble().click();

  // Then
  theConversationDialog().should("contain.text", "codex-fast · Find the parser");
  theConversationDialog().should("contain.text", "The parser is in src/parser.rs.");
  theConversationDialog().should("contain.text", "Read src/parser.rs");
});

it("a new frame in the conversation appears in the open dialog", () => {
  // Given
  const { backend, releaseConversation } = aSessionWithOneSubagentConversation({
    ...A_RECORDED_CONVERSATION,
    held: [replayAgentText("Fixed the off-by-one.", 3_000)],
  });
  mountTheSession(backend);
  theSubagentBubble().click();
  theConversationDialog().should("contain.text", "The parser is in src/parser.rs.");

  // When
  cy.then(() => releaseConversation());

  // Then
  theConversationDialog().should("contain.text", "Fixed the off-by-one.");
});

it("a tool call in the conversation opens its detail", () => {
  // Given
  const { backend } = aSessionWithOneSubagentConversation(A_RECORDED_CONVERSATION);
  mountTheSession(backend);
  theSubagentBubble().click();

  // When
  theConversationDialog().find('[data-message-kind="tool"]').contains("Read src/parser.rs").click();

  // Then
  agentActivityPage.detailInput().should("contain.text", "src/parser.rs");
});

it("an unrecorded conversation's bubble says it was not recorded", () => {
  // Given
  const { backend } = aSessionWithOneSubagentConversation("notRecorded");
  mountTheSession(backend);

  // When
  theSubagentBubble().click();

  // Then
  byTestId("subagent-conversation-not-recorded", { timeout: 5000 }).should("be.visible");
});
