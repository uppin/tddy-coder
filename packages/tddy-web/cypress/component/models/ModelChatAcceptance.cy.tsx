/**
 * Acceptance: opening Chat on a model starts an ACP session against that model and streams the
 * agent's reply into the chat pane — reusing `AcpService.Session` and `useAcpSession` rather than a
 * second chat implementation.
 *
 * The provider-backed ACP agent lives in `tddy-acp`; from the browser's side it is the same bidi
 * stream the pr-stack chat already speaks, so the same in-memory `AcpService` fake drives it.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md (AC10).
 */

import React from "react";
import { ModelLoadState, ModelRegistryService } from "../../../src/gen/models_pb";
import {
  AcpService,
  type AcpAgentMessage,
  type AcpClientMessage,
} from "../../../src/gen/tddy/acp/v1/acp_pb";
import { ModelsAppPage } from "../../../src/components/models/ModelsAppPage";
import { daemonRpcIdentity, type DaemonHost } from "../../../src/lib/participantRole";
import { withSelectedDaemon } from "../../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../../support/rpc/inMemory";
import {
  acpAgentChunk,
  acpPromptEnd,
  acpRecordingSession,
  acpScriptedSession,
  promptTexts,
} from "../../support/rpc/acpSession";
import {
  aModelRegistryBackend,
  anLlmModel,
  anOllamaProvider,
  FIXTURE_DAEMON,
} from "../../support/rpc/modelRegistryBackend";
import { modelsScreenPage as page, type ModelRef } from "../../support/pages/modelsScreenPage";
import { recordedFields } from "../../support/rpc/recordedRequests";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const FIXTURE_HOST: DaemonHost = {
  instanceId: FIXTURE_DAEMON,
  label: `${FIXTURE_DAEMON} (this daemon)`,
};

const QWEN: ModelRef = {
  daemonInstanceId: FIXTURE_DAEMON,
  providerId: "prov-ollama",
  modelId: "qwen3:32b",
};

/** The agent side of `AcpService.Session`: consumes client frames, yields agent frames. */
type AcpSessionHandler = (
  requests: AsyncIterable<AcpClientMessage>,
) => AsyncIterable<AcpAgentMessage>;

/** The registry the chat is opened from, with an ACP session handler layered on the same backend. */
function aChattableRegistry(
  session: AcpSessionHandler,
  loadState: ModelLoadState = ModelLoadState.LOADED,
) {
  return aModelRegistryBackend({
    providers: [anOllamaProvider()],
    models: [anLlmModel({ loadState })],
  }).implement(AcpService, { session });
}

/**
 * Mount with the model's owning daemon present in the common room — the state a chat is opened in.
 * The chat names that participant as the one serving its stream, so its presence is part of the
 * fixture rather than an implicit "some room".
 */
const mount = (backend: ReturnType<typeof aModelRegistryBackend>) =>
  mountWithRpc(
    withSelectedDaemon(<ModelsAppPage onNavigate={cy.stub()} />, [FIXTURE_HOST], [
      daemonRpcIdentity(FIXTURE_DAEMON),
    ]),
    backend,
  );

/** The same screen after that daemon has dropped out of the common room. */
const mountWithoutTheDaemonInTheRoom = (backend: ReturnType<typeof aModelRegistryBackend>) =>
  mountWithRpc(
    withSelectedDaemon(<ModelsAppPage onNavigate={cy.stub()} />, [FIXTURE_HOST], []),
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

describe("ModelChatAcceptance — chatting with a model over ACP", () => {
  it("streams the agent's reply into the chat transcript", () => {
    // Given — an ACP agent that answers with one message and ends the turn
    const backend = aChattableRegistry(
      acpScriptedSession(acpAgentChunk("Ollama here, ready."), acpPromptEnd()),
    );
    mount(backend);

    // When
    page.openChat(QWEN);

    // Then
    page.chatDialog().should("be.visible");
    page.chatTranscript().should("contain.text", "Ollama here, ready.");
  });

  it("sends the operator's prompt to the ACP agent", () => {
    // Given — a recording ACP session that captures what the client sends after the handshake
    const recorder = acpRecordingSession([acpAgentChunk("Ollama here, ready.")]);
    const backend = aChattableRegistry(recorder.session);
    mount(backend);

    // When
    page.openChat(QWEN);
    page.sendChatPrompt("How many parameters do you have?");

    // Then — the agent received the operator's words, not just some frame of the right kind
    cy.wrap(recorder).should((r) => {
      expect(r.sent).to.have.length(1);
      expect(promptTexts(r.sent[0])).to.deep.equal(["How many parameters do you have?"]);
    });
  });

  it("loads a model that is not resident before opening its chat", () => {
    // Given — the model is evicted when the operator asks to chat with it
    const backend = aChattableRegistry(
      acpScriptedSession(acpAgentChunk("Ollama here, ready."), acpPromptEnd()),
      ModelLoadState.NOT_LOADED,
    );
    mount(backend);

    // When
    page.openChat(QWEN);

    // Then — the daemon was asked to make it resident as part of starting the chat
    cy.wrap(backend).should((b) => {
      expect(recordedFields(b.callsTo(ModelRegistryService.method.loadModel))).to.deep.equal([
        { sessionToken: "fake-token", providerId: "prov-ollama", modelId: "qwen3:32b" },
      ]);
    });
    page.chatTranscript().should("contain.text", "Ollama here, ready.");
  });

  it("refuses a prompt once the model's daemon has left the common room", () => {
    // Given — a chat whose owning daemon is no longer a participant, so nothing is reading the
    // stream the prompt would be enqueued onto
    const recorder = acpRecordingSession([acpAgentChunk("Ollama here, ready.")]);
    const backend = aChattableRegistry(recorder.session);
    mountWithoutTheDaemonInTheRoom(backend);

    // When
    page.openChat(QWEN);
    page.sendChatPrompt("How many parameters do you have?");

    // Then — the operator is told the prompt did not go anywhere, and told which host is missing:
    // this chat has no "presenter", it has the daemon they picked off the Models screen. Reporting
    // success and echoing their own words back is the one answer that is never true
    page
      .chatError()
      .should("have.text", `Message not sent — daemon ${FIXTURE_DAEMON} is not connected.`);
    page.chatTranscript().should("not.contain.text", "How many parameters do you have?");
    cy.wrap(recorder).should((r) => {
      expect(r.sent).to.have.length(0);
    });
  });
});
