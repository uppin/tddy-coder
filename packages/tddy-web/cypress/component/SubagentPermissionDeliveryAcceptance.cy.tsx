/**
 * Acceptance: a permission request the main agent handed to the user is answered by the daemon that
 * owns the session it was parked in. The answer names that daemon, and an entry whose owning daemon
 * is not known offers no answer rather than sending one without it.
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
/** The daemon that owns the session in these fixtures: not the local default, so an answer that
 *  fell back to "" would be visibly wrong. */
const OWNING_DAEMON = "daemon-b";

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

/** A session whose subagent asks for a permission, served by a session that names
 *  `daemonInstanceId` as its owner (`""` when the owner is not known). */
function mountASessionWhoseSubagentAsks(options: {
  deferredToUser: boolean;
  daemonInstanceId: string;
}) {
  const { backend } = aConversationReplayBackend({
    session: {
      counts: [1],
      snapshot: [aBubble(options.deferredToUser ? ToolCallStatus.PENDING : ToolCallStatus.IN_PROGRESS)],
    },
    conversations: {
      [CONVERSATION]: {
        counts: [1],
        snapshot: [THE_PERMISSION_ENTRY],
        details: { "permission:perm-1": thePermissionDetail(options.deferredToUser) },
      },
    },
  });
  backend.onUnary(SessionAgentService.method.answerAgentPermission, () => ({ answeredBy: "alice" }));
  mountWithRpc(
    <div style={{ display: "flex", flexDirection: "column", height: 700, width: 1200 }}>
      <SessionWithActivitySidebar
        session={create(SessionEntrySchema, {
          sessionId: "managed-1",
          daemonInstanceId: options.daemonInstanceId,
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

it("a deferred request answered from the conversation names the daemon that owns the session", () => {
  // Given
  const backend = mountASessionWhoseSubagentAsks({
    deferredToUser: true,
    daemonInstanceId: OWNING_DAEMON,
  });
  openTheConversation();

  // When
  byTestId("permission-option-allow-once", { timeout: 5000 }).click();

  // Then
  cy.wrap(null).should(() => {
    const answers = recordedFields(backend.callsTo(SessionAgentService.method.answerAgentPermission));
    expect(answers).to.deep.equal([
      {
        sessionToken: "tok",
        sessionId: "managed-1",
        daemonInstanceId: OWNING_DAEMON,
        conversationId: CONVERSATION,
        requestId: "perm-1",
        optionId: "allow-once",
        cancelled: false,
      },
    ]);
  });
  byTestId("permission-answered").should("contain.text", "alice");
});

it("a deferred request whose owning daemon is not known offers no answer and says so", () => {
  // Given
  const backend = mountASessionWhoseSubagentAsks({ deferredToUser: true, daemonInstanceId: "" });
  openTheConversation();

  // Then
  byTestId("permission-cannot-answer", { timeout: 5000 }).should("contain.text", "cannot be answered");
  byTestId("permission-option-allow-once").should("not.exist");
  expect(backend.callsTo(SessionAgentService.method.answerAgentPermission)).to.have.length(0);
});
