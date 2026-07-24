/**
 * Acceptance: the **Agent Activity overlay as a read-only ACP transcript**. The overlay body is no
 * longer a flat tool-call list — it renders an ACP-style conversation (agent text interleaved with
 * enriched tool calls) fed by `ConnectionService.StreamAcpReplay`, a server-streaming RPC that emits
 * ACP-format frames over the HTTP client (works for live and dormant sessions alike).
 *
 * These mount the self-contained `AgentActivityOverlay` over an in-memory backend whose
 * `StreamAcpReplay` handler is an `async function*` yielding `AcpAgentMessage` frames, so they
 * exercise the real stream-subscription path without a live daemon.
 *
 * PRD: docs/ft/web/agent-activity-pane.md § Read-only ACP transcript.
 */

import React from "react";
import { AgentActivityOverlay } from "../../src/components/sessions/AgentActivityOverlay";
import { mountWithRpc } from "../support/rpc/inMemory";
import { agentActivityPage } from "../support/pages/agentActivityPage";
import { agentChatPage } from "../support/pages/agentChatPage";
import {
  backendReplaying,
  replayAgentText,
  replayToolCall,
  ToolCallStatus,
  ToolKind,
} from "../support/rpc/acpReplay";

function mountOverlay(backend: ReturnType<typeof backendReplaying>) {
  mountWithRpc(<AgentActivityOverlay sessionId="s1" sessionToken="tok" sessionType="tool" />, backend);
}

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("renders the agent's text output as an agent bubble", () => {
  // Given — a replay stream carrying one agent message
  const backend = backendReplaying(replayAgentText("Analyzing the parser.", 1_000));

  // When
  mountOverlay(backend);
  agentActivityPage.open();

  // Then — the transcript shows the agent's text as an agent bubble
  agentChatPage.chatMessage(0).should("have.text", "Analyzing the parser.");
  agentChatPage.chatMessageKind(0).should("equal", "agent");
});

it("labels a Read tool call with its file and line range", () => {
  // Given — a completed Read the server enriched into "Read main.rs L10-49" (name + file + window)
  const backend = backendReplaying(
    replayToolCall({
      id: "tool-1",
      title: "Read main.rs L10-49",
      kind: ToolKind.READ,
      status: ToolCallStatus.COMPLETED,
      input: { file_path: "src/main.rs", offset: 10, limit: 40 },
      atUnixMs: 1_000,
    }),
  );

  // When
  mountOverlay(backend);
  agentActivityPage.open();

  // Then — the tool entry names the tool and its file + line range, not a bare "Read"
  agentChatPage.chatMessageKind(0).should("equal", "tool");
  agentChatPage.chatMessage(0).should("contain.text", "Read");
  agentChatPage.chatMessage(0).should("contain.text", "main.rs");
  agentChatPage.chatMessage(0).should("contain.text", "L10-49");
});

it("marks an in-progress tool call as running", () => {
  // Given — a tool call still in progress
  const backend = backendReplaying(
    replayToolCall({
      id: "tool-1",
      title: "Bash",
      kind: ToolKind.EXECUTE,
      status: ToolCallStatus.IN_PROGRESS,
      input: { command: "cargo test --workspace", description: "run the tests" },
      atUnixMs: 1_000,
    }),
  );

  // When
  mountOverlay(backend);
  agentActivityPage.open();

  // Then — the entry is flagged running
  agentChatPage.chatToolStatus(0).should("contain.text", "running");
});

it("shows the elapsed time since the previous entry as a +Ns badge", () => {
  // Given — two entries exactly two seconds apart
  const backend = backendReplaying(
    replayAgentText("First.", 1_000),
    replayAgentText("Second.", 3_000),
  );

  // When
  mountOverlay(backend);
  agentActivityPage.open();

  // Then — the second entry's badge reads the 2s gap
  agentChatPage.chatElapsed(1).should("have.text", "+2s");
});

it("is read-only — offers no message input or send control", () => {
  // Given — a transcript with one entry
  const backend = backendReplaying(replayAgentText("Read-only.", 1_000));

  // When
  mountOverlay(backend);
  agentActivityPage.open();

  // Then — the message list is present but the composer is not
  agentChatPage.chatMessages().should("exist");
  agentChatPage.chatInput().should("not.exist");
  agentChatPage.chatSendBtn().should("not.exist");
});

it("renders the ACP transcript in place of the legacy tool-call row list", () => {
  // Given — a tool call streamed over the ACP replay
  const backend = backendReplaying(
    replayToolCall({
      id: "tool-1",
      title: "Read",
      kind: ToolKind.READ,
      status: ToolCallStatus.COMPLETED,
      input: { file_path: "src/lib.rs" },
      atUnixMs: 1_000,
    }),
  );

  // When
  mountOverlay(backend);
  agentActivityPage.open();

  // Then — the transcript renders; the old one-line record row does not
  agentChatPage.chat().should("exist");
  agentActivityPage.row("tool-1", { timeout: 1000 }).should("not.exist");
});

it("renders each streamed entry in order (agent text, tool call, agent text)", () => {
  // Given — an interleaved conversation
  const backend = backendReplaying(
    replayAgentText("Let me read the file.", 1_000),
    replayToolCall({
      id: "tool-1",
      title: "Read",
      kind: ToolKind.READ,
      status: ToolCallStatus.COMPLETED,
      input: { file_path: "src/main.rs", offset: 1, limit: 20 },
      atUnixMs: 2_000,
    }),
    replayAgentText("Now I understand it.", 3_000),
  );

  // When
  mountOverlay(backend);
  agentActivityPage.open();

  // Then — three entries render in the streamed order
  agentChatPage.chatMessageKind(0).should("equal", "agent");
  agentChatPage.chatMessageKind(1).should("equal", "tool");
  agentChatPage.chatMessageKind(2).should("equal", "agent");
  agentChatPage.chatMessage(2).should("have.text", "Now I understand it.");
});

it("coalesces a tool call's running then completed frames into one entry", () => {
  // Given — the persisted transcript emits the SAME tool_call_id twice as the call progresses:
  // first in-progress, then completed.
  const backend = backendReplaying(
    replayToolCall({
      id: "tool-1",
      title: "Bash cargo test",
      kind: ToolKind.EXECUTE,
      status: ToolCallStatus.IN_PROGRESS,
      input: { command: "cargo test" },
      atUnixMs: 1_000,
    }),
    replayToolCall({
      id: "tool-1",
      title: "Bash cargo test",
      kind: ToolKind.EXECUTE,
      status: ToolCallStatus.COMPLETED,
      input: { command: "cargo test" },
      atUnixMs: 3_000,
    }),
  );

  // When
  mountOverlay(backend);
  agentActivityPage.open();

  // Then — the two frames coalesce into a single tool entry carrying the terminal status; no second
  // entry is appended for the repeated id.
  agentChatPage.chatMessageKind(0).should("equal", "tool");
  agentChatPage.chatToolStatus(0).should("contain.text", "completed");
  agentChatPage.chatMessage(1).should("not.exist");
});
