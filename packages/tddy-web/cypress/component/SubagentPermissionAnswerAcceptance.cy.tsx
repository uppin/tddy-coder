/**
 * Acceptance: a permission request the main agent handed to the user is answered from the
 * subagent's conversation view — one button per option, the choice sent with
 * `AnswerAgentPermission`, and the bubble saying it waits for you meanwhile.
 *
 * Feature: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-web-approvals.md
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { createClient } from "@connectrpc/connect";
import { ActivityService } from "../../src/gen/activity_pb";
import { SessionAgentService } from "../../src/gen/session_agents_pb";
import { SessionEntrySchema } from "../../src/gen/session_pb";
import { SessionWithActivitySidebar } from "../../src/components/sessions/SessionWithActivitySidebar";
import { mountWithRpc } from "../support/rpc/inMemory";
import { recordedFields } from "../support/rpc/recordedRequests";
import { byTestId } from "../support/testIds";
import {
  ToolCallStatus,
  ToolKind,
  aConversationReplayBackend,
  aToolDetail,
  replayToolCall,
} from "../support/rpc/acpReplay";

const CONVERSATION = "conv-1";

function aBubble(status: ToolCallStatus) {
  return replayToolCall({
    id: `subagent:${CONVERSATION}`,
    title: "claude-coder · Run the tests",
    kind: ToolKind.OTHER,
    status,
    atUnixMs: 1_000,
  });
}

const THE_PERMISSION_ENTRY = replayToolCall({
  id: "permission:perm-1",
  title: "Permission: Run cargo test",
  kind: ToolKind.EXECUTE,
  status: ToolCallStatus.PENDING,
  atUnixMs: 2_000,
});

function thePermissionDetail(deferredToUser: boolean) {
  return aToolDetail({
    input: {
      requestId: "perm-1",
      title: "Run cargo test",
      options: [
        { optionId: "allow-once", name: "Allow once", kind: "allow_once" },
        { optionId: "reject-once", name: "Reject", kind: "reject_once" },
      ],
      deferredToUser,
    },
  });
}

function mountASessionWhoseSubagentAsks(deferredToUser: boolean) {
  const { backend } = aConversationReplayBackend({
    session: {
      counts: [1],
      snapshot: [aBubble(deferredToUser ? ToolCallStatus.PENDING : ToolCallStatus.IN_PROGRESS)],
    },
    conversations: {
      [CONVERSATION]: {
        counts: [1],
        snapshot: [THE_PERMISSION_ENTRY],
        details: { "permission:perm-1": thePermissionDetail(deferredToUser) },
      },
    },
  });
  backend.onUnary(SessionAgentService.method.answerAgentPermission, () => ({ answeredBy: "alice" }));
  mountWithRpc(
    <div style={{ display: "flex", flexDirection: "column", height: 700, width: 1200 }}>
      <SessionWithActivitySidebar
        session={create(SessionEntrySchema, {
          sessionId: "managed-1",
          daemonInstanceId: "daemon-a",
          isActive: true,
          status: "active",
          sessionType: "claude-cli",
          managedCodebase: true,
        })}
        sessionToken="tok"
        client={createClient(ActivityService, backend.transport())}
        suppressed={false}
        toolbar={(toggle) => <div>{toggle}</div>}
      >
        <div>terminal</div>
      </SessionWithActivitySidebar>
    </div>,
    backend,
  );
  return backend;
}

function theSubagentBubble() {
  return byTestId("agent-chat-messages").find('[data-message-kind="subagent"]');
}

function openTheConversation() {
  theSubagentBubble().click();
}

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("a deferred request shows one button per option", () => {
  // Given
  mountASessionWhoseSubagentAsks(true);

  // When
  openTheConversation();

  // Then
  byTestId("permission-option-allow-once", { timeout: 5000 }).should("contain.text", "Allow once");
  byTestId("permission-option-reject-once").should("contain.text", "Reject");
});

it("a request not deferred to the user shows no buttons", () => {
  // Given
  mountASessionWhoseSubagentAsks(false);

  // When
  openTheConversation();

  // Then
  byTestId("subagent-conversation-dialog", { timeout: 5000 }).should("contain.text", "Run cargo test");
  byTestId("permission-option-allow-once").should("not.exist");
});

it("choosing an option sends it and the entry records the choice", () => {
  // Given
  const backend = mountASessionWhoseSubagentAsks(true);
  openTheConversation();

  // When
  byTestId("permission-option-allow-once", { timeout: 5000 }).click();

  // Then
  cy.wrap(null).should(() => {
    const answers = recordedFields(backend.callsTo(SessionAgentService.method.answerAgentPermission)) as {
      conversationId: string;
      requestId: string;
      optionId: string;
    }[];
    expect(answers.map(({ conversationId, requestId, optionId }) => ({ conversationId, requestId, optionId })))
      .to.deep.equal([{ conversationId: CONVERSATION, requestId: "perm-1", optionId: "allow-once" }]);
  });
  byTestId("permission-answered").should("contain.text", "alice");
});

it("the bubble reads waiting for you while a request is deferred", () => {
  // When
  mountASessionWhoseSubagentAsks(true);

  // Then
  theSubagentBubble().should("contain.text", "waiting for you");
});
