/**
 * Acceptance: the Agent Chat "Export" button downloads a plain-text transcript with timestamps,
 * merging chat messages and clarification (elicitation) points into one chronological timeline — so
 * an operator can see what the agent did and when, including where it paused for input.
 *
 * PRD: docs/ft/coder/acp-protobuf-rpc.md.
 */

import React from "react";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { AgentChat } from "../../src/components/chat/AgentChat";
import { buildChatTranscript } from "../../src/components/chat/chatTranscript";
import type { ChatMessage, ElicitationPoint } from "../../src/components/chat/useAgentChat";
import { AcpService } from "../../src/gen/tddy/acp/v1/acp_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { acpAgentChunk, acpQuestion, acpScriptedSession } from "../support/rpc/acpSession";
import { agentChatPage } from "../support/pages/agentChatPage";

beforeEach(() => {
  cy.viewport(1280, 800);
});

// --- Pure transcript builder --------------------------------------------------------------------

it("builds a chronological timeline merging messages and clarification points with timestamps", () => {
  const messages: ChatMessage[] = [
    { key: "g0", from: "goal", text: "analyze-stack", at: 1_000 },
    { key: "a0", from: "agent", text: "Splitting the feature…", at: 3_000 },
    { key: "u0", from: "user", text: "Claude", at: 5_000 },
  ];
  const elicitations: ElicitationPoint[] = [
    {
      at: 4_000,
      kind: "select" as const,
      header: "Backend",
      question: "Which backend?",
      options: ["Claude", "Cursor"],
      allowOther: true,
    },
  ];

  const txt = buildChatTranscript(messages, elicitations);
  const lines = txt.split("\n").filter((l) => l.startsWith("["));

  // Chronological: goal(1s) → agent(3s) → clarification(4s) → user(5s)
  expect(lines[0]).to.contain("Goal: analyze-stack");
  expect(lines[1]).to.contain("Agent: Splitting the feature…");
  expect(lines[2]).to.contain("CLARIFICATION").and.to.contain("[Backend]").and.to.contain("Which backend?");
  expect(lines[3]).to.contain("You: Claude");
  // ISO timestamps present, and the clarification lists its options.
  expect(lines[0]).to.match(/^\[\d{4}-\d{2}-\d{2}T[\d:.]+Z\]/);
  expect(txt).to.contain("options: Claude | Cursor");
});

// --- Wired button (ACP transport) ---------------------------------------------------------------

it("downloads a .txt transcript containing the streamed message and the clarification", () => {
  const backend = anInMemoryRpcBackend().implement(AcpService, {
    session: acpScriptedSession(
      acpAgentChunk("Analyzing the feature into a PR stack.\n"),
      acpQuestion(["Claude", "Cursor"], { header: "Backend", question: "Which backend?" }),
    ),
  });

  mountWithRpc(<AgentChat room={null} acp placeholder="Message the agent…" />, backend);

  // Wait until the streamed line and the clarification have arrived.
  agentChatPage.chatMessage(0).should("contain.text", "Analyzing the feature into a PR stack.");
  agentChatPage.chatQuestion().should("exist");

  // Capture the downloaded blob instead of navigating, then assert its contents.
  cy.window().then((win) => {
    const captured: Blob[] = [];
    cy.stub(win.URL, "createObjectURL").callsFake((b: Blob) => {
      captured.push(b);
      return "blob:stub";
    });
    cy.stub(win.URL, "revokeObjectURL");
    cy.stub(win.HTMLAnchorElement.prototype, "click");

    agentChatPage.chatExportBtn().should("be.enabled").click();

    cy.wrap(null)
      .then(() => captured[0].text())
      .then((txt) => {
        expect(txt).to.contain("Agent: Analyzing the feature into a PR stack.");
        expect(txt).to.contain("CLARIFICATION").and.to.contain("Which backend?");
        expect(txt).to.contain("options: Claude | Cursor");
        expect(txt).to.match(/\[\d{4}-\d{2}-\d{2}T[\d:.]+Z\]/);
      });
  });
});
