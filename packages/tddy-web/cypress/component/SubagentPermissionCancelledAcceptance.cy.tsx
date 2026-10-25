/**
 * Acceptance: a permission request the transcript records as cancelled says who cancelled it, and
 * offers no option, however the conversation is reopened. A request answered with an option still
 * names the option and the user who chose it.
 *
 * Feature: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-web-approvals.md
 */

import React from "react";
import { createClient } from "@connectrpc/connect";
import { ActivityService } from "../../src/gen/activity_pb";
import { SessionEntrySchema } from "../../src/gen/session_pb";
import { create } from "@bufbuild/protobuf";
import { SessionWithActivitySidebar } from "../../src/components/sessions/SessionWithActivitySidebar";
import { mountWithRpc } from "../support/rpc/inMemory";
import { byTestId } from "../support/testIds";
import {
  ToolCallStatus,
  ToolKind,
  aConversationReplayBackend,
  aToolDetail,
  replayToolCall,
} from "../support/rpc/acpReplay";

const CONVERSATION = "conv-1";

function aBubble() {
  return replayToolCall({
    id: `subagent:${CONVERSATION}`,
    title: "claude-coder · Run the tests",
    kind: ToolKind.OTHER,
    status: ToolCallStatus.IN_PROGRESS,
    atUnixMs: 1_000,
  });
}

/** The request's transcript entry, completed: the transcript records its answer as its output. */
function thePermissionEntry() {
  return replayToolCall({
    id: "permission:perm-1",
    title: "Permission: Run cargo test",
    kind: ToolKind.EXECUTE,
    status: ToolCallStatus.COMPLETED,
    atUnixMs: 2_000,
  });
}

/** A session whose subagent asked for a permission the main agent handed to the user, and whose
 *  transcript records `answeredWith` as the request's answer. */
function mountASessionWithARecordedRequest(answeredWith: unknown) {
  const { backend } = aConversationReplayBackend({
    session: {
      counts: [1],
      snapshot: [aBubble()],
    },
    conversations: {
      [CONVERSATION]: {
        counts: [1],
        snapshot: [thePermissionEntry()],
        details: {
          "permission:perm-1": aToolDetail({
            input: {
              requestId: "perm-1",
              title: "Run cargo test",
              options: [
                { optionId: "allow-once", name: "Allow once", kind: "allow_once" },
                { optionId: "reject-once", name: "Reject", kind: "reject_once" },
              ],
              deferredToUser: true,
            },
            output: answeredWith,
          }),
        },
      },
    },
  });
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

it("a request the main agent cancelled names who cancelled it, with no option", () => {
  // Given
  mountASessionWithARecordedRequest({ answeredBy: "main-agent", cancelled: true });

  // When
  openTheConversation();

  // Then
  byTestId("permission-answered", { timeout: 5000 }).should("have.text", "Cancelled by main-agent");
  cy.get("[data-testid^='permission-option-']").should("not.exist");
});

it("a cancelled request still names who cancelled it, with no option, after a reopen", () => {
  // Given
  mountASessionWithARecordedRequest({ answeredBy: "main-agent", cancelled: true });
  openTheConversation();
  byTestId("permission-answered", { timeout: 5000 }).should("have.text", "Cancelled by main-agent");

  // When
  byTestId("subagent-conversation-close").click();
  byTestId("subagent-conversation-dialog").should("not.exist");
  openTheConversation();

  // Then
  byTestId("permission-answered", { timeout: 5000 }).should("have.text", "Cancelled by main-agent");
  cy.get("[data-testid^='permission-option-']").should("not.exist");
});

it("an option answer still names the option and the user who chose it", () => {
  // Given
  mountASessionWithARecordedRequest({ answeredBy: "alice", optionId: "allow-once" });

  // When
  openTheConversation();

  // Then
  byTestId("permission-answered", { timeout: 5000 }).should(
    "have.text",
    "Allow once · answered by alice",
  );
  cy.get("[data-testid^='permission-option-']").should("not.exist");
});
