/**
 * Acceptance: the reusable `AgentChat` component, mounted standalone (no `PrStackScreen`, no
 * sessions drawer), merges streamed agent output into ONE growing chat bubble — proving the
 * streaming reconstruction moved intact from `PrStackChat`/`usePresenterChat` into the extracted
 * `AgentChat`/`useAgentChat`.
 *
 * The presenter broadcasts raw `AgentOutput` chunks — token deltas as they stream, plus (for some
 * backends) a repeated full-line snapshot after the newline. `AgentChat` must reconcile these the
 * same way the TUI does (`AgentOutputActivityLogMerge`): accumulate into one line, finalize on
 * `\n`, dedup the repeat — so a sentence shows as a single bubble, streamed token-by-token, with no
 * per-token line breaks and no duplicated compound sentence.
 *
 * PRD: docs/ft/web/session-drawer.md § Agent Chat.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { AgentChat } from "../../src/components/chat/AgentChat";
import { TddyRemote, ServerMessageSchema } from "../../src/gen/tddy/v1/remote_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { agentChatPage } from "../support/pages/agentChatPage";

const SENTENCE = 'The feature request is only "hi".';

/**
 * Streams the sentence token-by-token (as the cursor backend does), then the terminating newline,
 * then the whole sentence again as one full-line snapshot — the exact double-emit that produced
 * per-token line breaks + a duplicated compound sentence before the merge was ported.
 */
async function* streamedTokensThenDuplicateFullLine() {
  const tokens = ["The", " feature", " request", " is", " only", ' "', "hi", '".'];
  for (const token of tokens) {
    yield create(ServerMessageSchema, {
      event: { case: "agentOutput", value: { text: token } },
    });
  }
  yield create(ServerMessageSchema, { event: { case: "agentOutput", value: { text: "\n" } } });
  yield create(ServerMessageSchema, {
    event: { case: "agentOutput", value: { text: `${SENTENCE}\n` } },
  });
}

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("merges streamed agent tokens into a single chat bubble with no per-token lines or duplicate", () => {
  // Given — a standalone AgentChat over a presenter stream that emits the sentence token-by-token,
  // then a duplicate full line (no sessions drawer, no PrStackScreen in the tree)
  const backend = anInMemoryRpcBackend().implement(TddyRemote, {
    stream: streamedTokensThenDuplicateFullLine,
    getSession: async () => ({}),
    listSessions: async () => ({ sessions: [] }),
  });

  // When
  mountWithRpc(<AgentChat room={null} placeholder="Message the agent…" />, backend);

  // Then — exactly one agent bubble holding the full sentence (tokens accumulated, not one-per-line)
  agentChatPage.chatMessage(0).should("exist").and("have.text", SENTENCE);
  // …and no second bubble (no per-token bubbles, no duplicated compound sentence)
  agentChatPage.chatMessage(1).should("not.exist");
});
