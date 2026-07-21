/**
 * Acceptance: streamed agent output must render as ONE growing chat bubble, not a new bubble per
 * token. The presenter broadcasts raw `AgentOutput` chunks — token deltas as they stream, plus (for
 * some backends) a repeated full-line snapshot after the newline. The TUI View reconciles these via
 * `AgentOutputActivityLogMerge` (accumulate into one line, finalize on `\n`, dedup the repeat); the
 * chat window must do the same so a sentence shows as a single line, streamed token-by-token, with
 * no per-token line breaks and no duplicated compound sentence.
 *
 * PRD: docs/ft/web/session-drawer.md § Per-Workflow Session Views (chat window).
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { TddyRemote } from "../../src/gen/tddy/v1/remote_pb";
import { AcpService } from "../../src/gen/tddy/acp/v1/acp_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { acpAgentChunk, acpScriptedSession } from "../support/rpc/acpSession";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";

const PR_STACK_SESSION = {
  sessionId: "pr-stack-streaming-0000-0000-0000-000000000040",
  createdAt: "2026-07-03T09:00:00Z",
  status: "idle",
  repoPath: "/home/dev/pr-stack-project",
  pid: 0,
  isActive: false,
  projectId: "proj-pr-stack-streaming",
  daemonInstanceId: "",
  workflowGoal: "",
  pendingElicitation: false,
  orchestratorSessionId: "",
  recipe: "pr-stack",
  stackPlanJson: "",
};

const SENTENCE = 'The feature request is only "hi".';

/**
 * Streams the sentence token-by-token (as the cursor backend does), then the terminating newline,
 * then the whole sentence again as one full-line snapshot — the exact double-emit that produced
 * per-token line breaks + a duplicated compound sentence in the UI. Over ACP each is an
 * `agent_message_chunk`.
 */
const STREAMED_TOKENS = ["The", " feature", " request", " is", " only", ' "', "hi", '".'];
const streamedTokensThenDuplicateFullLine = acpScriptedSession(
  ...STREAMED_TOKENS.map(acpAgentChunk),
  acpAgentChunk("\n"),
  acpAgentChunk(`${SENTENCE}\n`),
);

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

it("merges streamed agent tokens into a single chat bubble with no per-token lines or duplicate", () => {
  // Given — a presenter stream that emits the sentence token-by-token, then a duplicate full line
  const backend = aSessionsDrawerBackend([PR_STACK_SESSION])
    .implement(TddyRemote, {
      getSession: async () => ({}),
      listSessions: async () => ({ sessions: [] }),
    })
    .implement(AcpService, { session: streamedTokensThenDuplicateFullLine });

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // Then — exactly one agent bubble holding the full sentence (tokens accumulated, not one-per-line)
  prStackScreenPage.chatMessage(0).should("exist").and("have.text", SENTENCE);
  // …and no second bubble (no per-token bubbles, no duplicated compound sentence)
  prStackScreenPage.chatMessage(1).should("not.exist");
});
