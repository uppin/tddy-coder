/**
 * Acceptance: the tool-call **detail dialog** — its rendering and chrome. Clicking a tool-call entry
 * opens a dialog rendering the call's body (fetched on demand, see
 * `AgentActivityDetailLazyBodyAcceptance`) as **prettified, color-highlighted JSON**. Only tool-call
 * entries are interactive — agent text stays inert — and the dialog closes via its close control.
 *
 * The overlay body is the read-only ACP transcript; these mount it over an in-memory `StreamAcpReplay`
 * backend whose snapshot carries a **body-less** tool call, with the body served by the
 * `GetAcpToolCallDetail` lookup.
 *
 * PRD: docs/ft/web/agent-activity-pane.md § Persisted, lazily-counted activity (§3–§4).
 */

import React from "react";
import { AgentActivityOverlay } from "../../src/components/sessions/AgentActivityOverlay";
import { mountWithRpc } from "../support/rpc/inMemory";
import { agentActivityPage } from "../support/pages/agentActivityPage";
import {
  aReplayBackend,
  replayAgentText,
  replayToolCallStripped,
  ToolCallStatus,
  ToolKind,
  type ReplayBackendHandle,
} from "../support/rpc/acpReplay";

function mountOverlay(handle: ReplayBackendHandle, sessionId: string) {
  mountWithRpc(
    <AgentActivityOverlay sessionId={sessionId} sessionToken="tok" sessionType="tool" />,
    handle.backend,
  );
}

/** A body-less completed Bash tool call whose body is served by the lookup. */
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

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("color-highlights the detail JSON", () => {
  // Given — a tool call whose fetched input is shown in the dialog
  const handle = aReplayBackend({
    counts: [1],
    snapshot: [aStrippedBashCall()],
    details: { "tool-1": TOOL_1_BODY },
  });

  // When
  mountOverlay(handle, "detail-highlight");
  agentActivityPage.open();
  agentActivityPage.openDetail(0);

  // Then — the JSON is rendered as syntax-highlighted tokens (Prism wraps them in `.token` spans),
  // not a single plain-text blob
  agentActivityPage.jsonHighlight().should("exist");
  agentActivityPage.jsonHighlight().find(".token").should("exist");
});

it("does not open a dialog for a non-tool (agent text) entry", () => {
  // Given — a transcript whose only entry is agent text
  const handle = aReplayBackend({
    counts: [1],
    snapshot: [replayAgentText("Just some prose.", 1_000)],
  });

  // When — the operator clicks the agent-text bubble
  mountOverlay(handle, "detail-nontool");
  agentActivityPage.open();
  agentActivityPage.openDetail(0);

  // Then — no detail dialog opens (text entries are inert)
  agentActivityPage.detailDialog({ timeout: 1000 }).should("not.exist");
});

it("closes the detail dialog via its close control", () => {
  // Given — an open detail dialog
  const handle = aReplayBackend({
    counts: [1],
    snapshot: [aStrippedBashCall()],
    details: { "tool-1": TOOL_1_BODY },
  });
  mountOverlay(handle, "detail-close");
  agentActivityPage.open();
  agentActivityPage.openDetail(0);
  agentActivityPage.detailDialog().should("exist");

  // When — the close control is used
  agentActivityPage.closeDetail();

  // Then — the dialog is dismissed
  agentActivityPage.detailDialog({ timeout: 1000 }).should("not.exist");
});
