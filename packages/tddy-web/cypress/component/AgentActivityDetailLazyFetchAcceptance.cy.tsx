/**
 * Acceptance: **how** the tool-call detail dialog loads its bodies, now that `StreamAcpReplay` strips
 * them from the transcript and the dialog resolves them through the unary `GetAcpToolCallDetail`:
 *
 * - a **skeleton** stands in for each JSON body while the lookup is in flight (the dialog used to be
 *   synchronous, so an unresolved body was indistinguishable from an empty one);
 * - a resolved body is **cached per tool call** — reopening a completed row issues no second lookup;
 * - a **still-running** call is not cached (its output can still arrive) and says so explicitly;
 * - a failed lookup is reported **inline**, with `NOT_FOUND` (no such `tool_call_id` in the transcript)
 *   worded distinctly from a transport failure — the two the hosts deliberately keep apart.
 *
 * Feature doc: docs/ft/web/agent-activity-pane.md#rendering-an-unanswered-lookup-updated-2026-07-26
 */

import React from "react";
import { AgentActivityOverlay } from "../../src/components/sessions/AgentActivityOverlay";
import { mountWithRpc } from "../support/rpc/inMemory";
import { agentActivityPage } from "../support/pages/agentActivityPage";
import {
  aReplayBackend,
  aReplayBackendWithFailingDetail,
  aReplayBackendWithHeldDetail,
  aToolDetail,
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

/** A finished Bash call — metadata only, as the stripped stream delivers it. */
function aCompletedCall() {
  return replayToolCall({
    id: "tool-1",
    title: "Bash cargo test",
    kind: ToolKind.EXECUTE,
    status: ToolCallStatus.COMPLETED,
    atUnixMs: 1_000,
  });
}

/** The same call while it is still executing — its output does not exist yet. */
function aRunningCall() {
  return replayToolCall({
    id: "tool-1",
    title: "Bash cargo test",
    kind: ToolKind.EXECUTE,
    status: ToolCallStatus.IN_PROGRESS,
    atUnixMs: 1_000,
  });
}

const completedBodies = {
  "tool-1": aToolDetail({
    input: { command: "cargo test --workspace" },
    output: { exit_code: 0, stdout: "test result: ok. 42 passed" },
  }),
};

/** A running call's bodies: input only — the response's `raw_output` is legitimately absent. */
const runningBodies = {
  "tool-1": aToolDetail({ input: { command: "cargo test --workspace" } }),
};

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("shows a skeleton in place of the JSON while the detail lookup is in flight", () => {
  // Given — a backend whose body lookup is received but held open
  const { backend } = aReplayBackendWithHeldDetail({
    counts: [1],
    snapshot: [aCompletedCall()],
    details: completedBodies,
  });

  // When — the row is opened and the lookup does not answer
  mountOverlay(backend, "lazy-skeleton");
  agentActivityPage.open();
  agentActivityPage.openDetail(0);

  // Then — the dialog is open with a placeholder, and no JSON has rendered
  agentActivityPage.detailDialog().should("exist");
  agentActivityPage.detailSkeleton().should("exist");
  agentActivityPage.jsonHighlight({ timeout: 1000 }).should("not.exist");
});

it("replaces the skeleton with the fetched JSON once the lookup resolves", () => {
  // Given — a held lookup, with the dialog open and showing its placeholder
  const { backend, releaseDetail } = aReplayBackendWithHeldDetail({
    counts: [1],
    snapshot: [aCompletedCall()],
    details: completedBodies,
  });
  mountOverlay(backend, "lazy-resolve");
  agentActivityPage.open();
  agentActivityPage.openDetail(0);
  agentActivityPage.detailSkeleton().should("exist");

  // When — the lookup answers
  cy.then(() => releaseDetail());

  // Then — the placeholder gives way to the fetched JSON
  agentActivityPage.detailSkeleton({ timeout: 1000 }).should("not.exist");
  agentActivityPage.detailInput().should("contain.text", "cargo test --workspace");
  agentActivityPage.detailOutput().should("contain.text", "test result: ok. 42 passed");
});

it("serves a reopened completed call from cache without a second request", () => {
  // Given — a completed call whose bodies have been fetched once
  const { backend } = aReplayBackend({
    counts: [1],
    snapshot: [aCompletedCall()],
    details: completedBodies,
  });
  mountOverlay(backend, "lazy-cache-hit");
  agentActivityPage.open();
  agentActivityPage.openDetail(0);
  agentActivityPage.detailInput().should("contain.text", "cargo test --workspace");

  // When — the dialog is closed and the same row reopened
  agentActivityPage.closeDetail();
  agentActivityPage.openDetail(0);

  // Then — the bodies render again, from cache: still exactly one lookup
  agentActivityPage.detailInput().should("contain.text", "cargo test --workspace");
  cy.wrap(null).should(() => {
    expect(requestedToolCallIds(backend)).to.deep.equal(["tool-1"]);
  });
});

it("re-requests the detail when a still-running call is reopened", () => {
  // Given — a call still in progress, opened once (its output can still arrive, so a partial body
  // must not be cached for the rest of the session)
  const { backend } = aReplayBackend({
    counts: [1],
    snapshot: [aRunningCall()],
    details: runningBodies,
  });
  mountOverlay(backend, "lazy-cache-miss");
  agentActivityPage.open();
  agentActivityPage.openDetail(0);
  agentActivityPage.detailInput().should("contain.text", "cargo test --workspace");

  // When — the dialog is closed and the same row reopened
  agentActivityPage.closeDetail();
  agentActivityPage.openDetail(0);
  agentActivityPage.detailInput().should("contain.text", "cargo test --workspace");

  // Then — the body was fetched afresh
  cy.wrap(null).should(() => {
    expect(requestedToolCallIds(backend)).to.deep.equal(["tool-1", "tool-1"]);
  });
});

it("reports that a still-running call has no output yet", () => {
  // Given — a running call: the lookup returns input but no output
  const { backend } = aReplayBackend({
    counts: [1],
    snapshot: [aRunningCall()],
    details: runningBodies,
  });

  // When
  mountOverlay(backend, "lazy-no-output");
  agentActivityPage.open();
  agentActivityPage.openDetail(0);

  // Then — the absence is stated, not silently omitted
  agentActivityPage.detailInput().should("contain.text", "cargo test --workspace");
  agentActivityPage
    .detailNoOutput()
    .should("have.text", "No output yet — tool call still running.");
});

it("reports a tool call missing from the transcript as not found", () => {
  // Given — a transcript row whose id the lookup cannot resolve (the host answers NOT_FOUND)
  const { backend } = aReplayBackend({
    counts: [1],
    snapshot: [aCompletedCall()],
    details: {},
  });

  // When
  mountOverlay(backend, "lazy-not-found");
  agentActivityPage.open();
  agentActivityPage.openDetail(0);

  // Then — the dialog stays open and names this specific cause
  agentActivityPage.detailDialog().should("exist");
  agentActivityPage
    .detailError()
    .should("have.text", "This tool call is not in the session transcript.");
});

it("reports a failed detail lookup as an error", () => {
  // Given — a host that cannot serve the lookup at all
  const { backend } = aReplayBackendWithFailingDetail({
    counts: [1],
    snapshot: [aCompletedCall()],
  });

  // When
  mountOverlay(backend, "lazy-error");
  agentActivityPage.open();
  agentActivityPage.openDetail(0);

  // Then — the failure is reported inline, worded apart from the not-found case
  agentActivityPage.detailDialog().should("exist");
  agentActivityPage.detailError().should("have.text", "Could not load tool call details.");
});
