/**
 * Acceptance: `AgentChat` driven over the **ACP protobuf mirror** (`AcpService.Session`) instead of
 * `TddyRemote.Stream` — the `acp` prop. This proves the browser can speak ACP directly over the
 * (mocked) LiveKit session transport and that:
 *   1. streamed `AgentMessageChunk`s merge into ONE growing bubble (same reconciliation as the
 *      `TddyRemote` path — `useAcpSession` reuses `useAgentChat`'s merge), and
 *   2. an agent-initiated `request_permission` renders as a clarification whose selection round-trips
 *      back to the agent as a `RequestPermissionResponse`.
 *
 * PRD: docs/ft/coder/acp-protobuf-rpc.md.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { AgentChat } from "../../src/components/chat/AgentChat";
import {
  AcpService,
  AcpAgentMessageSchema,
  type AcpClientMessage,
} from "../../src/gen/tddy/acp/v1/acp_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { agentChatPage } from "../support/pages/agentChatPage";

const SENTENCE = 'The feature request is only "hi".';

/** One `session/update` frame carrying an `AgentMessageChunk` of `text`. */
function agentChunk(text: string) {
  return create(AcpAgentMessageSchema, {
    id: 0n,
    msg: {
      case: "sessionUpdate",
      value: {
        sessionId: { value: "s1" },
        update: {
          update: {
            case: "agentMessageChunk",
            value: { content: { block: { case: "text", value: { text } } } },
          },
        },
      },
    },
  });
}

/** Streams the sentence token-by-token, then the newline, then the whole sentence again — the
 *  same double-emit the `TddyRemote` streaming test uses, but as ACP `AgentMessageChunk`s. */
async function* streamedAcpTokensThenDuplicateFullLine() {
  const tokens = ["The", " feature", " request", " is", " only", ' "', "hi", '".'];
  for (const token of tokens) {
    yield agentChunk(token);
  }
  yield agentChunk("\n");
  yield agentChunk(`${SENTENCE}\n`);
}

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("merges streamed ACP AgentMessageChunks into a single chat bubble with no per-token lines or duplicate", () => {
  // Given — a standalone AgentChat in ACP mode over an AcpService.Session that streams the sentence
  const backend = anInMemoryRpcBackend().implement(AcpService, {
    session: streamedAcpTokensThenDuplicateFullLine,
  });

  // When
  mountWithRpc(<AgentChat room={null} acp placeholder="Message the agent…" />, backend);

  // Then — exactly one agent bubble holding the full sentence (tokens accumulated, not one-per-line)
  agentChatPage.chatMessage(0).should("exist").and("have.text", SENTENCE);
  agentChatPage.chatMessage(1).should("not.exist");
});

it("renders an agent request_permission as a clarification and rounds the selection back to the agent", () => {
  // Given — an AcpService.Session that asks a permission question, then confirms whichever option
  // the client selects (proving the reply reached the agent).
  async function* session(requests: AsyncIterable<AcpClientMessage>) {
    yield create(AcpAgentMessageSchema, {
      id: 7n,
      msg: {
        case: "requestPermission",
        value: {
          sessionId: { value: "s1" },
          toolCall: { toolCallId: { value: "clarification" } },
          options: [
            { optionId: { value: "option-0" }, name: "Claude" },
            { optionId: { value: "option-1" }, name: "Cursor" },
          ],
        },
      },
    });
    // Skip the client's eager initialize / new_session; wait for its permission reply.
    for await (const msg of requests) {
      if (msg.msg.case === "requestPermission") {
        const selected = msg.msg.value.outcome?.outcome;
        const optionId =
          selected?.case === "selected" ? selected.value.optionId?.value : "(none)";
        yield agentChunk(`Using ${optionId}`);
        return;
      }
    }
  }

  const backend = anInMemoryRpcBackend().implement(AcpService, { session });

  // When
  mountWithRpc(<AgentChat room={null} acp placeholder="Message the agent…" />, backend);

  // Then — the clarification renders with one option per choice
  agentChatPage.chatQuestion().should("exist");
  agentChatPage.chatOption(0).should("have.text", "Claude");
  agentChatPage.chatOption(1).should("have.text", "Cursor");

  // When — the operator picks the first option
  agentChatPage.chatOption(0).click();

  // Then — the chosen label is echoed as the user's bubble, and the agent's confirmation (proving the
  // RequestPermissionResponse reached it, carrying option-0) streams back.
  agentChatPage.chatMessages().should("contain.text", "Claude");
  agentChatPage.chatMessages().should("contain.text", "Using option-0");
});
