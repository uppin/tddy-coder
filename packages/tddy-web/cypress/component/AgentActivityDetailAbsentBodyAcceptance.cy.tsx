/**
 * Acceptance: what the detail dialog says when a body is **legitimately absent** rather than
 * unfetched. Both `raw_input` and `raw_output` are `optional` on `GetAcpToolCallDetailResponse`, and a
 * transcript entry may carry no `tool_call_id` at all, so "nothing to show" has three distinct causes
 * the operator must be able to tell apart — none of which may render as an empty JSON block or borrow
 * the wording of a lookup failure.
 *
 * The sibling suites cover the fetch itself (`AgentActivityDetailLazyFetchAcceptance`) and the rendered
 * JSON (`AgentActivityDetailDialogAcceptance`).
 *
 * Feature doc: docs/ft/web/agent-activity-pane.md#rendering-an-unanswered-lookup-updated-2026-07-26
 */

import React from "react";
import { AgentActivityOverlay } from "../../src/components/sessions/AgentActivityOverlay";
import { mountWithRpc } from "../support/rpc/inMemory";
import { agentActivityPage } from "../support/pages/agentActivityPage";
import {
  aReplayBackend,
  aToolDetail,
  aToolDetailWithoutInput,
  replayToolCall,
  requestedToolCallIds,
  ToolCallStatus,
  ToolKind,
} from "../support/rpc/acpReplay";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";

function mountOverlay(backend: InMemoryRpcBackend, sessionId: string) {
  mountWithRpc(
    <AgentActivityOverlay sessionId={sessionId} sessionToken="tok" sessionType="tool" />,
    backend,
  );
}

/** A finished call — the status that makes "no output" permanent rather than pending. */
function aCompletedCall(id: string) {
  return replayToolCall({
    id,
    title: "Bash cargo test",
    kind: ToolKind.EXECUTE,
    status: ToolCallStatus.COMPLETED,
    atUnixMs: 1_000,
  });
}

/** A tool-call frame with no `tool_call_id` — the hosts emit one for a call they could not attribute,
 *  and the transcript still shows the row. */
function anUnattributedCall() {
  return replayToolCall({
    id: "",
    title: "Bash cargo test",
    kind: ToolKind.EXECUTE,
    status: ToolCallStatus.COMPLETED,
    atUnixMs: 1_000,
  });
}

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("reports a settled call's missing output as recorded rather than pending", () => {
  // Given — a completed call whose lookup resolves with an input but no output: it will never produce
  // one, so the running call's "not yet" wording would be a false promise
  const { backend } = aReplayBackend({
    counts: [1],
    snapshot: [aCompletedCall("tool-1")],
    details: { "tool-1": aToolDetail({ input: { command: "cargo test --workspace" } }) },
  });

  // When
  mountOverlay(backend, "absent-output-settled");
  agentActivityPage.open();
  agentActivityPage.openDetail(0);

  // Then
  agentActivityPage.detailInput().should("contain.text", "cargo test --workspace");
  agentActivityPage
    .detailNoOutput()
    .should("have.text", "No output recorded for this tool call.");
});

it("states an absent input instead of rendering an empty JSON block", () => {
  // Given — a call whose lookup resolves with an output but no input
  const { backend } = aReplayBackend({
    counts: [1],
    snapshot: [aCompletedCall("tool-1")],
    details: { "tool-1": aToolDetailWithoutInput({ output: { exit_code: 0 } }) },
  });

  // When
  mountOverlay(backend, "absent-input");
  agentActivityPage.open();
  agentActivityPage.openDetail(0);

  // Then — the absence is stated, and no empty highlighted block stands in for it
  agentActivityPage
    .detailNoInput()
    .should("have.text", "No input recorded for this tool call.");
  agentActivityPage.detailInput({ timeout: 1000 }).should("not.exist");
  agentActivityPage.detailOutput().should("contain.text", "exit_code");
});

it("tells the operator an entry carrying no tool call id has no bodies to show", () => {
  // Given — a transcript row whose frame carried no `tool_call_id`
  const { backend } = aReplayBackend({
    counts: [1],
    snapshot: [anUnattributedCall()],
    details: {},
  });

  // When
  mountOverlay(backend, "absent-id");
  agentActivityPage.open();
  agentActivityPage.openDetail(0);

  // Then — worded apart from a failed lookup, because nothing was ever asked
  agentActivityPage
    .detailError()
    .should(
      "have.text",
      "This activity entry has no tool call id, so its input and output are unavailable.",
    );
});

it("issues no lookup for an entry carrying no tool call id", () => {
  // Given — the same unattributable row
  const { backend } = aReplayBackend({
    counts: [1],
    snapshot: [anUnattributedCall()],
    details: {},
  });

  // When — the row is opened and its message rendered
  mountOverlay(backend, "absent-id-no-request");
  agentActivityPage.open();
  agentActivityPage.openDetail(0);
  agentActivityPage.detailError().should("exist");

  // Then — the dialog never asked the host for an empty id
  cy.wrap(null).should(() => {
    expect(requestedToolCallIds(backend)).to.deep.equal([]);
  });
});
