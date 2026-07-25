/**
 * Acceptance: clicking a **tool-call** entry in the Agent Activity transcript opens a **detail
 * dialog** rendering the call's `raw_input` and `raw_output` as **prettified, color-highlighted
 * JSON** (Requirement #3). Only tool-call entries are interactive — agent text stays inert.
 *
 * The overlay body is the read-only ACP transcript; these mount it over an in-memory `StreamAcpReplay`
 * backend whose snapshot carries a tool call with both input and output JSON.
 *
 * PRD: docs/ft/web/agent-activity-pane.md § Persisted, lazily-counted activity (§3).
 */

import React from "react";
import { AgentActivityOverlay } from "../../src/components/sessions/AgentActivityOverlay";
import { mountWithRpc } from "../support/rpc/inMemory";
import { agentActivityPage } from "../support/pages/agentActivityPage";
import {
  aReplayBackend,
  replayAgentText,
  replayToolCall,
  ToolCallStatus,
  ToolKind,
} from "../support/rpc/acpReplay";

type Backend = ReturnType<typeof aReplayBackend>["backend"];

function mountOverlay(backend: Backend, sessionId: string) {
  mountWithRpc(
    <AgentActivityOverlay sessionId={sessionId} sessionToken="tok" sessionType="tool" />,
    backend,
  );
}

/** A completed Bash tool call carrying both input and output JSON. */
function aBashCall() {
  return replayToolCall({
    id: "tool-1",
    title: "Bash cargo test",
    kind: ToolKind.EXECUTE,
    status: ToolCallStatus.COMPLETED,
    input: { command: "cargo test --workspace", description: "run the tests" },
    output: { exit_code: 0, stdout: "test result: ok. 42 passed" },
    atUnixMs: 1_000,
  });
}

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("opens a detail dialog showing the tool call's input JSON when its entry is clicked", () => {
  // Given — a transcript with one completed tool call
  const { backend } = aReplayBackend({ counts: [1], snapshot: [aBashCall()] });

  // When — the overlay is opened and the tool entry clicked
  mountOverlay(backend, "detail-input");
  agentActivityPage.open();
  agentActivityPage.detailDialog({ timeout: 1000 }).should("not.exist");
  agentActivityPage.openDetail(0);

  // Then — the dialog shows the full input as JSON
  agentActivityPage.detailDialog().should("exist");
  agentActivityPage.detailInput().should("contain.text", "command");
  agentActivityPage.detailInput().should("contain.text", "cargo test --workspace");
});

it("shows the tool call's output JSON in the detail dialog", () => {
  // Given — the same tool call (input + output)
  const { backend } = aReplayBackend({ counts: [1], snapshot: [aBashCall()] });

  // When
  mountOverlay(backend, "detail-output");
  agentActivityPage.open();
  agentActivityPage.openDetail(0);

  // Then — the output block renders the result JSON
  agentActivityPage.detailOutput().should("contain.text", "exit_code");
  agentActivityPage.detailOutput().should("contain.text", "test result: ok. 42 passed");
});

it("color-highlights the detail JSON", () => {
  // Given — a tool call whose input is shown in the dialog
  const { backend } = aReplayBackend({ counts: [1], snapshot: [aBashCall()] });

  // When
  mountOverlay(backend, "detail-highlight");
  agentActivityPage.open();
  agentActivityPage.openDetail(0);

  // Then — the JSON is rendered as syntax-highlighted tokens (Prism wraps them in `.token` spans),
  // not a single plain-text blob
  agentActivityPage.jsonHighlight().should("exist");
  agentActivityPage.jsonHighlight().find(".token").should("exist");
});

it("does not open a dialog for a non-tool (agent text) entry", () => {
  // Given — a transcript whose only entry is agent text
  const { backend } = aReplayBackend({
    counts: [1],
    snapshot: [replayAgentText("Just some prose.", 1_000)],
  });

  // When — the operator clicks the agent-text bubble
  mountOverlay(backend, "detail-nontool");
  agentActivityPage.open();
  agentActivityPage.openDetail(0);

  // Then — no detail dialog opens (text entries are inert)
  agentActivityPage.detailDialog({ timeout: 1000 }).should("not.exist");
});

it("closes the detail dialog via its close control", () => {
  // Given — an open detail dialog
  const { backend } = aReplayBackend({ counts: [1], snapshot: [aBashCall()] });
  mountOverlay(backend, "detail-close");
  agentActivityPage.open();
  agentActivityPage.openDetail(0);
  agentActivityPage.detailDialog().should("exist");

  // When — the close control is used
  agentActivityPage.closeDetail();

  // Then — the dialog is dismissed
  agentActivityPage.detailDialog({ timeout: 1000 }).should("not.exist");
});
