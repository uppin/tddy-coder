/**
 * Acceptance: clicking a **tool-call** entry in the Agent Activity transcript opens a **detail
 * dialog** rendering the call's `raw_input` and `raw_output` as **prettified, color-highlighted
 * JSON** (Requirement #3). Only tool-call entries are interactive — agent text stays inert.
 *
 * The bodies are **not** in the streamed transcript: `StreamAcpReplay` strips them from every frame,
 * and the dialog resolves the clicked call's bodies through the unary `GetAcpToolCallDetail`. These
 * specs mount the overlay over an in-memory backend that models exactly that split — body-less
 * frames on the stream, bodies behind the lookup.
 *
 * Feature doc: docs/ft/web/agent-activity-pane.md#rendering-an-unanswered-lookup-updated-2026-07-26
 * PRD: docs/ft/web/agent-activity-pane.md § Persisted, lazily-counted activity (§3).
 */

import React from "react";
import { AgentActivityOverlay } from "../../src/components/sessions/AgentActivityOverlay";
import { mountWithRpc } from "../support/rpc/inMemory";
import { agentActivityPage } from "../support/pages/agentActivityPage";
import {
  aReplayBackend,
  aToolDetail,
  replayAgentText,
  replayToolCall,
  requestedToolCallIds,
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

/** A completed Bash tool call — metadata only, exactly as the stripped stream delivers it. */
function aBashCall() {
  return replayToolCall({
    id: "tool-1",
    title: "Bash cargo test",
    kind: ToolKind.EXECUTE,
    status: ToolCallStatus.COMPLETED,
    atUnixMs: 1_000,
  });
}

/** That call's bodies, as the `GetAcpToolCallDetail` lookup resolves them. */
const bashBodies = {
  "tool-1": aToolDetail({
    input: { command: "cargo test --workspace", description: "run the tests" },
    output: { exit_code: 0, stdout: "test result: ok. 42 passed" },
  }),
};

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("fetches the clicked tool call's input JSON and renders it", () => {
  // Given — a transcript with one completed tool call whose bodies live behind the lookup
  const { backend } = aReplayBackend({
    counts: [1],
    snapshot: [aBashCall()],
    details: bashBodies,
  });

  // When — the overlay is opened and the tool entry clicked
  mountOverlay(backend, "detail-input");
  agentActivityPage.open();
  agentActivityPage.detailDialog({ timeout: 1000 }).should("not.exist");
  agentActivityPage.openDetail(0);

  // Then — the dialog shows the fetched input as JSON
  agentActivityPage.detailDialog().should("exist");
  agentActivityPage.detailInput().should("contain.text", "command");
  agentActivityPage.detailInput().should("contain.text", "cargo test --workspace");
});

it("renders the fetched output JSON for a completed call", () => {
  // Given — the same tool call (input + output behind the lookup)
  const { backend } = aReplayBackend({
    counts: [1],
    snapshot: [aBashCall()],
    details: bashBodies,
  });

  // When
  mountOverlay(backend, "detail-output");
  agentActivityPage.open();
  agentActivityPage.openDetail(0);

  // Then — the output block renders the result JSON
  agentActivityPage.detailOutput().should("contain.text", "exit_code");
  agentActivityPage.detailOutput().should("contain.text", "test result: ok. 42 passed");
});

it("color-highlights the fetched detail JSON", () => {
  // Given — a tool call whose fetched input is shown in the dialog
  const { backend } = aReplayBackend({
    counts: [1],
    snapshot: [aBashCall()],
    details: bashBodies,
  });

  // When
  mountOverlay(backend, "detail-highlight");
  agentActivityPage.open();
  agentActivityPage.openDetail(0);

  // Then — the JSON is rendered as syntax-highlighted tokens (Prism wraps them in `.token` spans),
  // not a single plain-text blob
  agentActivityPage.jsonHighlight().should("exist");
  agentActivityPage.jsonHighlight().find(".token").should("exist");
});

it("requests the detail for the clicked row's tool call id", () => {
  // Given — a transcript of two distinct tool calls, each with its own bodies
  const { backend } = aReplayBackend({
    counts: [2],
    snapshot: [
      aBashCall(),
      replayToolCall({
        id: "tool-2",
        title: "Read main.rs",
        kind: ToolKind.READ,
        status: ToolCallStatus.COMPLETED,
        atUnixMs: 2_000,
      }),
    ],
    details: {
      ...bashBodies,
      "tool-2": aToolDetail({ input: { file_path: "src/main.rs" } }),
    },
  });

  // When — the operator opens the SECOND row
  mountOverlay(backend, "detail-id");
  agentActivityPage.open();
  agentActivityPage.openDetail(1);
  agentActivityPage.detailInput().should("contain.text", "src/main.rs");

  // Then — exactly that row's id was looked up, not the first row's
  cy.wrap(null).should(() => {
    expect(requestedToolCallIds(backend)).to.deep.equal(["tool-2"]);
  });
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
  const { backend } = aReplayBackend({
    counts: [1],
    snapshot: [aBashCall()],
    details: bashBodies,
  });
  mountOverlay(backend, "detail-close");
  agentActivityPage.open();
  agentActivityPage.openDetail(0);
  agentActivityPage.detailDialog().should("exist");

  // When — the close control is used
  agentActivityPage.closeDetail();

  // Then — the dialog is dismissed
  agentActivityPage.detailDialog({ timeout: 1000 }).should("not.exist");
});
