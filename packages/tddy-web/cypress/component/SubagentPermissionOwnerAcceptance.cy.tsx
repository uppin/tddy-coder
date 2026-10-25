/**
 * Acceptance: a permission request is answered by the daemon that holds the conversation it was
 * parked in. For a split session that is the codebase half, not the session's own daemon, and the
 * answer names that half's session.
 *
 * Feature: docs/ft/web/agent-activity-pane.md § The user answers a deferred permission request
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { createClient } from "@connectrpc/connect";
import { ActivityService } from "../../src/gen/activity_pb";
import {
  AnswerAgentPermissionResponseSchema,
  SessionAgentService,
} from "../../src/gen/session_agents_pb";
import { SessionEntrySchema, type SessionEntry } from "../../src/gen/session_pb";
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

/** Where a session is held: its own daemon and id, and — when split — the codebase half's. */
interface Placement {
  sessionId: string;
  daemonInstanceId: string;
  codebaseSessionId?: string;
  codebaseDaemonInstanceId?: string;
}

function aSession(placement: Placement): SessionEntry {
  return create(SessionEntrySchema, {
    codebaseSessionId: "",
    codebaseDaemonInstanceId: "",
    isActive: true,
    status: "active",
    sessionType: "claude-cli",
    managedCodebase: true,
    ...placement,
  });
}

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

const THE_PERMISSION_DETAIL = aToolDetail({
  input: {
    requestId: "perm-1",
    title: "Run cargo test",
    options: [
      { optionId: "allow-once", name: "Allow once", kind: "allow_once" },
      { optionId: "reject-once", name: "Reject", kind: "reject_once" },
    ],
    deferredToUser: true,
  },
});

/** A session whose subagent has a deferred permission request waiting for the user. */
function mountASessionWhoseSubagentAsks(session: SessionEntry) {
  const { backend } = aConversationReplayBackend({
    session: {
      counts: [1],
      snapshot: [aBubble(ToolCallStatus.PENDING)],
    },
    conversations: {
      [CONVERSATION]: {
        counts: [1],
        snapshot: [THE_PERMISSION_ENTRY],
        details: { "permission:perm-1": THE_PERMISSION_DETAIL },
      },
    },
  });
  backend.onUnary(SessionAgentService.method.answerAgentPermission, () =>
    create(AnswerAgentPermissionResponseSchema, { answeredBy: "alice" }),
  );
  mountWithRpc(
    <div style={{ display: "flex", flexDirection: "column", height: 700, width: 1200 }}>
      <SessionWithActivitySidebar
        session={session}
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

it("a split session's request is answered by the codebase half that holds the conversation", () => {
  // Given
  const backend = mountASessionWhoseSubagentAsks(
    aSession({
      sessionId: "managed-1",
      daemonInstanceId: "daemon-a",
      codebaseSessionId: "codebase-1",
      codebaseDaemonInstanceId: "daemon-codebase",
    }),
  );
  openTheConversation();

  // When
  byTestId("permission-option-allow-once", { timeout: 5000 }).click();

  // Then
  cy.wrap(null).should(() => {
    const answers = recordedFields(
      backend.callsTo(SessionAgentService.method.answerAgentPermission),
    );
    expect(answers).to.deep.equal([
      {
        sessionToken: "tok",
        sessionId: "codebase-1",
        daemonInstanceId: "daemon-codebase",
        conversationId: CONVERSATION,
        requestId: "perm-1",
        optionId: "allow-once",
        cancelled: false,
      },
    ]);
  });
  byTestId("permission-answered").should("contain.text", "alice");
});

it("a co-located session's request is answered by its own daemon and session", () => {
  // Given
  const backend = mountASessionWhoseSubagentAsks(
    aSession({ sessionId: "managed-1", daemonInstanceId: "daemon-a" }),
  );
  openTheConversation();

  // When
  byTestId("permission-option-allow-once", { timeout: 5000 }).click();

  // Then
  cy.wrap(null).should(() => {
    const answers = recordedFields(
      backend.callsTo(SessionAgentService.method.answerAgentPermission),
    );
    expect(answers).to.deep.equal([
      {
        sessionToken: "tok",
        sessionId: "managed-1",
        daemonInstanceId: "daemon-a",
        conversationId: CONVERSATION,
        requestId: "perm-1",
        optionId: "allow-once",
        cancelled: false,
      },
    ]);
  });
  byTestId("permission-answered").should("contain.text", "alice");
});
