/**
 * Acceptance: the **Agent Activity pane** — a per-session, top-bar overlay that lists the agent's
 * own tool calls (streamed live via `ConnectionService.StreamSessionActivity`), each expandable
 * into a scrollable full-input/output detail dialog.
 *
 * These tests mount the self-contained `AgentActivityOverlay` over an in-memory backend whose
 * `StreamSessionActivity` handler is an `async function*` (same server-streaming shape as the ACP
 * mirror in `AgentChatAcpStreamingAcceptance.cy.tsx`), so they exercise the real stream-subscription
 * path without a live daemon.
 *
 * PRD: docs/ft/web/agent-activity-pane.md.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { ConnectionService, AgentActivityRecordSchema } from "../../src/gen/connection_pb";
import { AgentActivityOverlay } from "../../src/components/sessions/AgentActivityOverlay";
import { mountWithRpc } from "../support/rpc/inMemory";
import { agentActivityPage } from "../support/pages/agentActivityPage";

/** Build one `AgentActivityRecord` for the stream, showing only the fields a test cares about. */
function activityRecord(fields: {
  callId: string;
  toolName: string;
  inputJson?: string;
  status?: "running" | "completed" | "error";
  resultJson?: string;
  errorMessage?: string;
  source?: string;
}) {
  return create(AgentActivityRecordSchema, {
    callId: fields.callId,
    toolName: fields.toolName,
    inputJson: fields.inputJson ?? "{}",
    status: fields.status ?? "completed",
    resultJson: fields.resultJson ?? "",
    errorMessage: fields.errorMessage ?? "",
    startedUnixMs: 0n,
    completedUnixMs: 0n,
    source: fields.source ?? "coder",
  });
}

/** A backend whose `StreamSessionActivity` replays exactly `records`, then stays quiet. */
function backendStreaming(...records: ReturnType<typeof activityRecord>[]) {
  return anInMemoryRpcBackend().implement(ConnectionService, {
    // eslint-disable-next-line require-yield
    async *streamSessionActivity() {
      for (const record of records) {
        yield record;
      }
    },
  });
}

function mountOverlay(
  backend: ReturnType<typeof anInMemoryRpcBackend>,
  props?: { sessionType?: string },
) {
  mountWithRpc(
    <AgentActivityOverlay
      sessionId="s1"
      sessionToken="tok"
      sessionType={props?.sessionType ?? "tool"}
    />,
    backend,
  );
}

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("hides the activity icon when the session has no tool-call activity", () => {
  // Given — a session whose activity stream yields no records
  const backend = backendStreaming();

  // When
  mountOverlay(backend);

  // Then — no icon is offered
  agentActivityPage.button().should("not.exist");
});

it("shows the activity icon once the first tool call is streamed", () => {
  // Given — a stream carrying one tool call
  const backend = backendStreaming(activityRecord({ callId: "call-1", toolName: "Bash" }));

  // When
  mountOverlay(backend);

  // Then — the icon appears
  agentActivityPage.button().should("exist");
});

it("opens the activity overlay listing a one-line row for each tool call", () => {
  // Given — a stream carrying two tool calls
  const backend = backendStreaming(
    activityRecord({ callId: "call-1", toolName: "Bash" }),
    activityRecord({ callId: "call-2", toolName: "Read" }),
  );

  // When
  mountOverlay(backend);
  agentActivityPage.open();

  // Then — the overlay lists one row per call, labelled with the tool name
  agentActivityPage.overlay().should("exist");
  agentActivityPage.row("call-1").should("contain.text", "Bash");
  agentActivityPage.row("call-2").should("contain.text", "Read");
});

it("opens a detail dialog with the full input and full output when a record is selected", () => {
  // Given — a completed tool call with distinctive input and output
  const backend = backendStreaming(
    activityRecord({
      callId: "call-1",
      toolName: "Bash",
      inputJson: '{"command":"cargo test --workspace"}',
      resultJson: '{"stdout":"test result: ok. 412 passed","exit_code":0}',
    }),
  );

  // When
  mountOverlay(backend);
  agentActivityPage.open();
  agentActivityPage.openDetail("call-1");

  // Then — the dialog shows the full input and full output
  agentActivityPage.detailDialog().should("exist");
  agentActivityPage.detailInput().should("contain.text", "cargo test --workspace");
  agentActivityPage.detailOutput().should("contain.text", "test result: ok. 412 passed");
});

it("closes the detail dialog on Escape", () => {
  // Given — the detail dialog is open for a record
  const backend = backendStreaming(activityRecord({ callId: "call-1", toolName: "Bash" }));
  mountOverlay(backend);
  agentActivityPage.open();
  agentActivityPage.openDetail("call-1");
  agentActivityPage.detailDialog().should("exist");

  // When — Escape is pressed
  cy.get("body").type("{esc}");

  // Then — the dialog is dismissed
  agentActivityPage.detailDialog().should("not.exist");
});

it("closes the detail dialog on a backdrop click", () => {
  // Given — the detail dialog is open for a record
  const backend = backendStreaming(activityRecord({ callId: "call-1", toolName: "Bash" }));
  mountOverlay(backend);
  agentActivityPage.open();
  agentActivityPage.openDetail("call-1");
  agentActivityPage.detailDialog().should("exist");

  // When — the backdrop (dialog's parent overlay) is clicked outside the dialog box
  agentActivityPage.detailDialog().parent().click("topLeft");

  // Then — the dialog is dismissed
  agentActivityPage.detailDialog().should("not.exist");
});

it("flags newly streamed activity with an unread badge until the overlay is opened", () => {
  // Given — a stream that emits one call now and a second only after the test releases it
  let releaseSecond: () => void = () => undefined;
  const secondReleased = new Promise<void>((resolve) => {
    releaseSecond = resolve;
  });
  const backend = anInMemoryRpcBackend().implement(ConnectionService, {
    async *streamSessionActivity() {
      yield activityRecord({ callId: "call-1", toolName: "Bash" });
      await secondReleased;
      yield activityRecord({ callId: "call-2", toolName: "Read" });
    },
  });

  // When — the component mounts and the first call arrives
  mountOverlay(backend);

  // Then — the freshly-arrived activity is flagged unread
  agentActivityPage.unreadBadge().should("exist");

  // When — the operator opens the overlay
  agentActivityPage.open();

  // Then — the badge clears (the current activity is now seen)
  agentActivityPage.unreadBadge().should("not.exist");

  // When — the overlay is closed and a new call streams in
  agentActivityPage.close();
  cy.then(() => releaseSecond());

  // Then — the new activity is flagged unread again
  agentActivityPage.unreadBadge().should("exist");
});

it("renders the activity pane for a sandbox session (session-type-agnostic)", () => {
  // Given — a sandbox session whose agent tool call is streamed with a sandbox source
  const backend = backendStreaming(
    activityRecord({ callId: "call-1", toolName: "Read", source: "sandbox" }),
  );

  // When
  mountOverlay(backend, { sessionType: "sandbox" });
  agentActivityPage.open();

  // Then — the icon and record render exactly as for any other session type
  agentActivityPage.button().should("exist");
  agentActivityPage.row("call-1").should("contain.text", "Read");
});
