/**
 * Acceptance tests: the PR-Stack Chat Screen's chat window implements a remote Presenter over
 * the existing bidirectional `TddyRemote.Stream` RPC — inbound `PresenterEvent`s render as chat
 * bubbles, and user input is sent as `UserIntent`s (`ClientMessage` oneof).
 *
 * PRD: docs/ft/web/session-drawer.md § Per-Workflow Session Views (chat window).
 * Changeset: docs/dev/1-WIP/pr-stack-workflow-views.md.
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { PrStackChat } from "../../src/components/sessions/prstack/PrStackChat";
import { TddyRemote } from "../../src/gen/tddy/v1/remote_pb";
import { AcpService, type AcpClientMessage } from "../../src/gen/tddy/acp/v1/acp_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { acpAgentChunk, acpRecordingSession, acpScriptedSession } from "../support/rpc/acpSession";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const PR_STACK_SESSION = {
  sessionId: "pr-stack-session-3333-0000-0000-0000-000000000030",
  createdAt: "2026-07-01T09:00:00Z",
  status: "idle",
  repoPath: "/home/dev/pr-stack-project",
  pid: 0,
  isActive: false,
  projectId: "proj-pr-stack",
  daemonInstanceId: "",
  workflowGoal: "",
  pendingElicitation: false,
  orchestratorSessionId: "",
  recipe: "pr-stack",
  stackPlanJson: "",
};

/** Streams a single agent message chunk, then idles — the request stream is ignored. */
const oneAgentOutputMessage = acpScriptedSession(
  acpAgentChunk("Analyzing the feature into a PR stack…"),
);

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

it("renders an agent output event from the presenter stream as a chat bubble", () => {
  // Given
  const chatBackend = aSessionsDrawerBackend([PR_STACK_SESSION])
    .implement(TddyRemote, {
      getSession: async () => ({}),
      listSessions: async () => ({ sessions: [] }),
    })
    .implement(AcpService, { session: oneAgentOutputMessage });

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), chatBackend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // Then
  prStackScreenPage.chat().should("exist");
  prStackScreenPage
    .chatMessage(0)
    .should("exist")
    .and("contain.text", "Analyzing the feature into a PR stack…");
});

/** The text of a `prompt` `AcpClientMessage`'s first content block. */
function promptText(m: AcpClientMessage): string {
  if (m.msg.case !== "prompt") return `<${m.msg.case}>`;
  const block = m.msg.value.prompt[0]?.block;
  return block?.case === "text" ? block.value.text : "";
}

function aRecordingChatBackend() {
  const { session, sent } = acpRecordingSession();
  const backend = aSessionsDrawerBackend([PR_STACK_SESSION])
    .implement(TddyRemote, {
      getSession: async () => ({}),
      listSessions: async () => ({ sessions: [] }),
    })
    .implement(AcpService, { session });
  return { backend, sentIntents: sent };
}

it("sends the operator's first message to the agent as an ACP prompt", () => {
  // Over ACP the client always sends a `prompt`; the agent maps the first prompt of a fresh session
  // to SubmitFeatureInput and later ones to QueuePrompt (pinned in `convert_acp`'s unit tests).
  const { backend, sentIntents } = aRecordingChatBackend();

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();
  prStackScreenPage.sendChatMessage("Split the auth feature into a PR stack.");

  // Then
  cy.wrap(sentIntents).should((sent) => {
    expect(sent).to.have.length(1);
    expect(sent[0].msg.case).to.equal("prompt");
    expect(promptText(sent[0])).to.equal("Split the auth feature into a PR stack.");
  });
});

it("sends a follow-up message as a second ACP prompt while the workflow is running", () => {
  const { backend, sentIntents } = aRecordingChatBackend();
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();
  prStackScreenPage.sendChatMessage("Split the auth feature into a PR stack.");

  // When — the operator sends a follow-up nudge
  prStackScreenPage.sendChatMessage("Split this into three PRs instead of two.");

  // Then
  cy.wrap(sentIntents).should((sent) => {
    expect(sent).to.have.length(2);
    expect(sent[1].msg.case).to.equal("prompt");
    expect(promptText(sent[1])).to.equal("Split this into three PRs instead of two.");
  });
});

it("renders the chat panel when no LiveKit room has been attached to the session yet", () => {
  // Given — a pr-stack session before any LiveKit room is connected: the real production
  // state today, since nothing yet threads a connected orchestrator Room through to this
  // screen (see the TODO in usePresenterChat.ts). No RpcTransportProvider override here —
  // this deliberately exercises the real default LiveKit transport factory, not the
  // in-memory fake the other tests in this file use, to catch a factory that assumes a
  // non-null room.

  // When
  cy.mount(<PrStackChat session={PR_STACK_SESSION as any} room={null} livekitServerIdentity="server" />);

  // Then
  prStackScreenPage.chat().should("exist");
});
