/**
 * Acceptance: an assistant carries usage notes — how to use it, its quirks, its limitations —
 * written by the operator in the create dialog, editable in the edit dialog, and shown on its
 * row in the panel.
 *
 * Usage notes are **operator documentation, never machine context**: they are stored on the
 * assistant and displayed to humans, and are deliberately not injected into any system prompt
 * and not advertised to a main agent at session open (PRD scope decision).
 *
 * PRD: docs/dev/1-WIP/2026-09-27-agent-usage-notes-prd.md (AC3, AC4).
 */

import React from "react";
import { ModelRegistryService } from "../../../src/gen/models_pb";
import { ModelsAppPage } from "../../../src/components/models/ModelsAppPage";
import type { DaemonHost } from "../../../src/lib/participantRole";
import { withSelectedDaemon } from "../../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../../support/rpc/inMemory";
import {
  aModelRegistryBackend,
  anAssistant,
  anLlmModel,
  anOllamaProvider,
  FIXTURE_DAEMON,
} from "../../support/rpc/modelRegistryBackend";
import {
  modelsScreenPage as page,
  type AssistantRef,
  type ModelRef,
} from "../../support/pages/modelsScreenPage";
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

const REPO_READER: AssistantRef = { daemonInstanceId: FIXTURE_DAEMON, name: "repo-reader" };

const USAGE_NOTES =
  "Best at refactoring seams, bad at API design. Needs maxTurns: 5 or it wanders. " +
  "Don't ask it for summaries — it reads everything first.";

const mount = (backend: ReturnType<typeof aModelRegistryBackend>) =>
  mountWithRpc(
    withSelectedDaemon(<ModelsAppPage onNavigate={cy.stub()} />, [FIXTURE_HOST]),
    backend,
  );

const aRegistryWithOneModel = () =>
  aModelRegistryBackend({ providers: [anOllamaProvider()], models: [anLlmModel()] });

/** A registry holding an assistant that already carries usage notes. */
const aRegistryWithANotedAssistant = () =>
  aModelRegistryBackend({
    providers: [anOllamaProvider()],
    models: [anLlmModel()],
    assistants: [anAssistant({ usageNotes: USAGE_NOTES })],
  });

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 900);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  cy.then(() => window.localStorage.setItem("tddy_session_token", "fake-token"));
});

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

describe("AssistantUsageNotesAcceptance — an agent's notes for its operator", () => {
  it("submits the usage notes the operator wrote alongside the assistant's definition", () => {
    // Given
    const backend = aRegistryWithOneModel();
    mount(backend);

    // When — the operator writes notes while composing the assistant
    page.openCreateAssistant(QWEN);
    page.fillAndSubmitCreateAssistantForm({
      name: "repo-reader",
      label: "Repo Reader",
      systemPrompt: "You read code and answer questions about it.",
      tools: ["Read", "Grep"],
      replaces: [],
    });
    cy.get('[data-testid="models-create-assistant-usage-notes"]')
      .should("exist")
      .clear()
      .type(USAGE_NOTES);

    // Then — the notes reach the daemon with the definition
    cy.wrap(backend).should((b) => {
      expect(
        recordedFields(b.callsTo(ModelRegistryService.method.createAssistant))[0]?.usageNotes,
      ).to.equal(USAGE_NOTES);
    });
  });

  it("pre-fills the edit dialog with the stored notes and updates them", () => {
    // Given
    const backend = aRegistryWithANotedAssistant();
    mount(backend);

    // When — the operator edits the notes down to a warning
    page.openEditAssistant(REPO_READER);
    cy.get('[data-testid="models-edit-assistant-usage-notes"]')
      .should("have.value", USAGE_NOTES)
      .clear()
      .type("Needs maxTurns: 5 or it wanders.");
    page.fillAndSubmitEditAssistantForm({
      label: 'Repo Reader',
      systemPrompt: 'You read code and answer questions about it.',
      tools: ['Read', 'Grep'],
      replaces: [],
    });

    // Then — the whole notes replace what was there, carried whole like `replaces`
    cy.wrap(backend).should((b) => {
      expect(
        recordedFields(b.callsTo(ModelRegistryService.method.updateAssistant))[0]?.usageNotes,
      ).to.equal("Needs maxTurns: 5 or it wanders.");
    });
  });

  it("shows the stored notes on the assistant's row in the panel", () => {
    // Given
    mount(aRegistryWithANotedAssistant());

    // Then — the row shows the notes, so an operator choosing an agent reads them before
    // prompting it
    page.assistantRow(REPO_READER).should("contain.text", "Best at refactoring seams");
  });
});
