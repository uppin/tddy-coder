/**
 * Acceptance: the Models & Agents dialogs are dismissed the way every other tddy-web modal is —
 * Escape or a press on the backdrop — and announce themselves as modal.
 *
 * A dialog that can only be left through its own Cancel button is the odd one out on this screen,
 * and one that never sets `aria-modal` leaves a screen reader offering the table behind it as
 * though it were still reachable.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md (AC8, AC10).
 */

import React from "react";
import { ModelLoadState } from "../../../src/gen/models_pb";
import { AcpService } from "../../../src/gen/tddy/acp/v1/acp_pb";
import { ModelsAppPage } from "../../../src/components/models/ModelsAppPage";
import { daemonRpcIdentity, type DaemonHost } from "../../../src/lib/participantRole";
import { withSelectedDaemon } from "../../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../../support/rpc/inMemory";
import { acpAgentChunk, acpPromptEnd, acpScriptedSession } from "../../support/rpc/acpSession";
import {
  aModelRegistryBackend,
  anLlmModel,
  anOllamaProvider,
  FIXTURE_DAEMON,
} from "../../support/rpc/modelRegistryBackend";
import { modelsScreenPage as page, type ModelRef } from "../../support/pages/modelsScreenPage";

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

/** A registry with one resident, chat-capable model, and an ACP agent behind it. */
function aChattableRegistry() {
  return aModelRegistryBackend({
    providers: [anOllamaProvider()],
    models: [anLlmModel({ loadState: ModelLoadState.LOADED })],
  }).implement(AcpService, {
    session: acpScriptedSession(acpAgentChunk("Ollama here, ready."), acpPromptEnd()),
  });
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

describe("ModelsDialogDismissAcceptance — leaving a Models & Agents dialog", () => {
  it("closes the create-assistant dialog on Escape", () => {
    // Given
    mount(aChattableRegistry());
    page.openCreateAssistant(QWEN);
    page.createAssistantDialog().should("be.visible");

    // When
    page.pressEscape();

    // Then
    page.createAssistantDialog().should("not.exist");
  });

  it("closes the create-assistant dialog when the backdrop is pressed", () => {
    // Given
    mount(aChattableRegistry());
    page.openCreateAssistant(QWEN);
    page.createAssistantDialog().should("be.visible");

    // When
    page.pressBackdropOf(page.createAssistantDialog);

    // Then
    page.createAssistantDialog().should("not.exist");
  });

  it("keeps the create-assistant dialog open when a press lands inside it", () => {
    // Given
    mount(aChattableRegistry());
    page.openCreateAssistant(QWEN);

    // When — a press that starts on the dialog itself, as a drag across a text field does
    page.pressInsideCreateAssistantDialog();

    // Then — dismissing on any press would throw away a half-written system prompt
    page.createAssistantDialog().should("be.visible");
  });

  it("announces the create-assistant dialog as modal", () => {
    // Given
    mount(aChattableRegistry());

    // When
    page.openCreateAssistant(QWEN);

    // Then
    page.createAssistantDialog().should("have.attr", "aria-modal", "true");
  });

  it("closes the chat dialog on Escape", () => {
    // Given
    mount(aChattableRegistry());
    page.openChat(QWEN);
    page.chatDialog().should("be.visible");

    // When
    page.pressEscape();

    // Then
    page.chatDialog().should("not.exist");
  });

  it("announces the chat dialog as modal", () => {
    // Given
    mount(aChattableRegistry());

    // When
    page.openChat(QWEN);

    // Then
    page.chatDialog().should("have.attr", "aria-modal", "true");
  });
});
