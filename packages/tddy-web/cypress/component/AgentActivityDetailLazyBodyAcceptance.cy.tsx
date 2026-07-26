/**
 * Acceptance: the Agent Activity transcript stream no longer inlines a tool call's body — it carries
 * only `title`/`status`/`tool_call_id` (the server strips `raw_input`/`raw_output`, PR #345). When an
 * operator clicks a tool-call row, the detail dialog **fetches that one call's body on demand** via
 * `GetAcpToolCallDetail`, showing a loading state while the fetch is in flight and an error state if
 * it fails. Fetched bodies are cached per `(sessionId, callId)`, so re-opening the same row does not
 * re-fetch.
 *
 * These mount the overlay over an in-memory backend whose `StreamAcpReplay` snapshot carries a
 * body-less tool call and whose `GetAcpToolCallDetail` serves (or withholds, or fails) the body.
 *
 * PRD: docs/ft/web/agent-activity-pane.md § 4 Lazy tool bodies — fetch on click.
 */

import React from "react";
import { ConnectionService } from "../../src/gen/connection_pb";
import { AgentActivityOverlay } from "../../src/components/sessions/AgentActivityOverlay";
import { mountWithRpc } from "../support/rpc/inMemory";
import { agentActivityPage } from "../support/pages/agentActivityPage";
import {
  aReplayBackend,
  replayToolCallStripped,
  ToolCallStatus,
  ToolKind,
  type ReplayBackendHandle,
} from "../support/rpc/acpReplay";

/** One body-less completed Bash tool call in the streamed transcript. Its body lives only behind the
 *  on-demand `GetAcpToolCallDetail` lookup, not on the frame. */
function aStrippedBashCall() {
  return replayToolCallStripped({
    id: "tool-1",
    title: "Bash cargo test",
    kind: ToolKind.EXECUTE,
    status: ToolCallStatus.COMPLETED,
    atUnixMs: 1_000,
  });
}

/** The body the lookup serves for `tool-1`. */
const TOOL_1_BODY = {
  rawInput: JSON.stringify({ command: "cargo test --workspace", description: "run the tests" }),
  rawOutput: JSON.stringify({ exit_code: 0, stdout: "test result: ok. 42 passed" }),
};

function mountOverlay(handle: ReplayBackendHandle, sessionId: string) {
  mountWithRpc(
    <AgentActivityOverlay sessionId={sessionId} sessionToken="tok" sessionType="tool" />,
    handle.backend,
  );
}

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("fetches the clicked tool call's input from GetAcpToolCallDetail and shows it", () => {
  // Given — a body-less streamed tool call, its body available only via the on-demand lookup
  const handle = aReplayBackend({
    counts: [1],
    snapshot: [aStrippedBashCall()],
    details: { "tool-1": TOOL_1_BODY },
  });

  // When — the overlay is opened and the tool entry clicked
  mountOverlay(handle, "lazy-input");
  agentActivityPage.open();
  agentActivityPage.openDetail(0);

  // Then — the dialog fills with the fetched input JSON
  agentActivityPage.detailInput().should("contain.text", "command");
  agentActivityPage.detailInput().should("contain.text", "cargo test --workspace");
});

it("shows the fetched output JSON in the detail dialog", () => {
  // Given
  const handle = aReplayBackend({
    counts: [1],
    snapshot: [aStrippedBashCall()],
    details: { "tool-1": TOOL_1_BODY },
  });

  // When
  mountOverlay(handle, "lazy-output");
  agentActivityPage.open();
  agentActivityPage.openDetail(0);

  // Then
  agentActivityPage.detailOutput().should("contain.text", "exit_code");
  agentActivityPage.detailOutput().should("contain.text", "test result: ok. 42 passed");
});

it("requests exactly the clicked call's body, keyed by its tool_call_id", () => {
  // Given
  const handle = aReplayBackend({
    counts: [1],
    snapshot: [aStrippedBashCall()],
    details: { "tool-1": TOOL_1_BODY },
  });

  // When
  mountOverlay(handle, "lazy-request");
  agentActivityPage.open();
  agentActivityPage.openDetail(0);
  agentActivityPage.detailInput().should("contain.text", "cargo test --workspace");

  // Then — one lookup was made, for that row's id and session
  cy.then(() => {
    const calls = handle.backend.callsTo(ConnectionService.method.getAcpToolCallDetail);
    expect(calls).to.have.length(1);
    expect(calls[0].toolCallId).to.equal("tool-1");
    expect(calls[0].sessionId).to.equal("lazy-request");
  });
});

it("shows a loading state while the body fetch is in flight, then the fetched body", () => {
  // Given — the lookup response is withheld until the spec releases it
  const handle = aReplayBackend({
    counts: [1],
    snapshot: [aStrippedBashCall()],
    details: { "tool-1": TOOL_1_BODY },
    holdDetail: true,
  });

  // When — the tool entry is clicked (starting the held fetch)
  mountOverlay(handle, "lazy-loading");
  agentActivityPage.open();
  agentActivityPage.openDetail(0);

  // Then — the dialog shows a loading state until the response is released, then the body
  agentActivityPage.detailLoading().should("exist");
  cy.then(() => handle.releaseDetail());
  agentActivityPage.detailInput().should("contain.text", "cargo test --workspace");
  agentActivityPage.detailLoading({ timeout: 1000 }).should("not.exist");
});

it("shows an error state when the body lookup fails", () => {
  // Given — no body registered for the call, so the lookup answers NOT_FOUND
  const handle = aReplayBackend({
    counts: [1],
    snapshot: [aStrippedBashCall()],
    details: {},
  });

  // When
  mountOverlay(handle, "lazy-error");
  agentActivityPage.open();
  agentActivityPage.openDetail(0);

  // Then — the dialog shows an error rather than an empty or loading body
  agentActivityPage.detailError().should("exist");
  agentActivityPage.detailInput({ timeout: 1000 }).should("not.exist");
});

it("reuses the cached body on re-open without a second fetch", () => {
  // Given — a fetched-and-shown body
  const handle = aReplayBackend({
    counts: [1],
    snapshot: [aStrippedBashCall()],
    details: { "tool-1": TOOL_1_BODY },
  });
  mountOverlay(handle, "lazy-cache");
  agentActivityPage.open();
  agentActivityPage.openDetail(0);
  agentActivityPage.detailInput().should("contain.text", "cargo test --workspace");

  // When — the dialog is closed and the same row re-opened
  agentActivityPage.closeDetail();
  agentActivityPage.detailDialog({ timeout: 1000 }).should("not.exist");
  agentActivityPage.openDetail(0);

  // Then — the body shows again from cache, with no second lookup
  agentActivityPage.detailInput().should("contain.text", "cargo test --workspace");
  cy.then(() => {
    const calls = handle.backend.callsTo(ConnectionService.method.getAcpToolCallDetail);
    expect(calls).to.have.length(1);
  });
});
