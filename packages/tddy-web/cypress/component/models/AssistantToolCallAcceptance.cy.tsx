/**
 * Acceptance: an assistant's tool calls surface in its chat — that one ran, what it produced, and
 * that a failed one failed.
 *
 * The agent reports each call twice: a `tool_call` announcing it, then a `tool_call_update` carrying
 * its terminal status and, as `raw_output`, whatever the tool answered
 * (`tddy-acp::provider_agent::dispatch`). Rendering only the first leaves a call that ran, one still
 * running and one that failed reading identically.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md (§ ACP chat — tool calls surface as
 * ACP `tool_call` / `tool_call_update` session updates).
 */

import React from "react";
import { ConnectionService } from "../../../src/gen/connection_pb";
import { AcpService, type AcpAgentMessage } from "../../../src/gen/tddy/acp/v1/acp_pb";
import { ModelsAppPage } from "../../../src/components/models/ModelsAppPage";
import { daemonRpcIdentity, type DaemonHost } from "../../../src/lib/participantRole";
import { withSelectedDaemon } from "../../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../../support/rpc/inMemory";
import {
  acpPromptEnd,
  acpScriptedSession,
  acpToolCall,
  acpToolCallResult,
} from "../../support/rpc/acpSession";
import {
  aModelRegistryBackend,
  anAssistant,
  anLlmModel,
  anOllamaProvider,
  aProject,
  FIXTURE_DAEMON,
  listedProjects,
} from "../../support/rpc/modelRegistryBackend";
import {
  modelsScreenPage as page,
  type AssistantRef,
} from "../../support/pages/modelsScreenPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const FIXTURE_HOST: DaemonHost = {
  instanceId: FIXTURE_DAEMON,
  label: `${FIXTURE_DAEMON} (this daemon)`,
};

const REPO_READER: AssistantRef = { daemonInstanceId: FIXTURE_DAEMON, name: "repo-reader" };

/** The one checkout the assistant's daemon will run its tools in. */
const TDDY_CODER = aProject();

/** A registry with one tool-bearing assistant, whose chat is scripted to emit `frames`. */
function aChattingAssistant(...frames: AcpAgentMessage[]) {
  return aModelRegistryBackend({
    providers: [anOllamaProvider()],
    models: [anLlmModel()],
    assistants: [anAssistant({ tools: ["Read", "Grep"] })],
  })
    .implement(AcpService, { session: acpScriptedSession(...frames) })
    .onUnary(ConnectionService.method.listProjects, () => listedProjects([TDDY_CODER]));
}

/** Open the assistant's chat in its daemon's only checkout. */
function openTheChat() {
  page.openAssistantChat(REPO_READER);
  page.chooseWorkspace("proj-1");
}

const mount = (backend: ReturnType<typeof aModelRegistryBackend>) =>
  mountWithRpc(
    withSelectedDaemon(<ModelsAppPage onNavigate={cy.stub()} />, [FIXTURE_HOST], [
      daemonRpcIdentity(FIXTURE_DAEMON),
    ]),
    backend,
  );

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  // Seed inside `cy.then` so it runs *after* the queued clears above; a bare synchronous
  // `setItem` executes first and is then wiped, leaving the screen with no session token.
  cy.then(() => window.localStorage.setItem("tddy_session_token", "fake-token"));
});

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

describe("AssistantToolCallAcceptance — an assistant's tool calls in its chat", () => {
  it("shows a completed tool call and what it produced", () => {
    // Given — the agent runs Grep and the tool answers
    mount(
      aChattingAssistant(
        acpToolCall("call-1", "Grep"),
        acpToolCallResult("call-1", { failed: false, output: "3 matches in src/main.rs" }),
        acpPromptEnd(),
      ),
    );

    // When
    openTheChat();

    // Then
    page.chatToolStatus(0).should("equal", "completed");
    page.chatMessage(0).should("contain.text", "Grep");
    page.chatMessage(0).should("contain.text", "3 matches in src/main.rs");
  });

  it("shows a failed tool call as failed, with what it reported", () => {
    // Given — the tool refused
    mount(
      aChattingAssistant(
        acpToolCall("call-1", "Read"),
        acpToolCallResult("call-1", { failed: true, output: "no such file: src/missing.rs" }),
        acpPromptEnd(),
      ),
    );

    // When
    openTheChat();

    // Then — a failure that renders like a success is worse than one that renders as nothing
    page.chatToolStatus(0).should("equal", "error");
    page.chatMessage(0).should("contain.text", "no such file: src/missing.rs");
  });

  it("claims no outcome for a tool call whose result has not arrived", () => {
    // Given — the agent announced the call and the tool has not answered yet
    mount(aChattingAssistant(acpToolCall("call-1", "Grep")));

    // When
    openTheChat();

    // Then — the call is on screen, with nothing said about how it ended. The announcement alone
    // reports only that the agent asked for it
    page.chatMessage(0).should("contain.text", "Grep");
    page.chatToolStatusMarker(0, { timeout: 0 }).should("not.exist");
  });

  it("keeps one bubble per tool call rather than a second one for its result", () => {
    // Given
    mount(
      aChattingAssistant(
        acpToolCall("call-1", "Grep"),
        acpToolCallResult("call-1", { failed: false, output: "3 matches in src/main.rs" }),
        acpPromptEnd(),
      ),
    );

    // When
    openTheChat();

    // Then — the waiting status assertion proves the update has been folded in, so the absence
    // below cannot pass merely because it had not arrived yet
    page.chatToolStatus(0).should("equal", "completed");
    page.chatMessage(1, { timeout: 0 }).should("not.exist");
  });

  it("keeps two tool calls apart, each with its own outcome", () => {
    // Given — one call succeeds, the next fails
    mount(
      aChattingAssistant(
        acpToolCall("call-1", "Grep"),
        acpToolCallResult("call-1", { failed: false, output: "3 matches in src/main.rs" }),
        acpToolCall("call-2", "Read"),
        acpToolCallResult("call-2", { failed: true, output: "no such file: src/missing.rs" }),
        acpPromptEnd(),
      ),
    );

    // When
    openTheChat();

    // Then — coalescing both onto one row would report the pair as whichever ended last
    page.chatToolStatus(0).should("equal", "completed");
    page.chatToolStatus(1).should("equal", "error");
    page.chatMessage(0).should("contain.text", "3 matches in src/main.rs");
    page.chatMessage(1).should("contain.text", "no such file: src/missing.rs");
  });
});
