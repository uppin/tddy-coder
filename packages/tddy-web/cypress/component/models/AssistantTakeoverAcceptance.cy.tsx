/**
 * Acceptance: an assistant is defined with **two** tool sets, and the daemon is sent both.
 *
 * `tools` is the assistant's own loop — what it may call while it works. `replaces` is what it
 * takes over from the *main* agent: the tools a session stops being able to call itself once this
 * assistant is attached, which is the only thing that makes the main agent delegate to it rather
 * than keep searching on its own. One picker writing both fields cannot express an assistant that
 * greps for itself without taking Grep away from everyone else, so the screen offers one picker per
 * question and each writes only its own field.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md (AC8, AC9).
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

const mount = (backend: ReturnType<typeof aModelRegistryBackend>) =>
  mountWithRpc(
    withSelectedDaemon(<ModelsAppPage onNavigate={cy.stub()} />, [FIXTURE_HOST]),
    backend,
  );

const aRegistryWithOneModel = () =>
  aModelRegistryBackend({ providers: [anOllamaProvider()], models: [anLlmModel()] });

/** A registry holding an assistant that already takes the main agent's Grep over. */
const aRegistryWithAGrepTakeover = () =>
  aModelRegistryBackend({
    providers: [anOllamaProvider()],
    models: [anLlmModel()],
    assistants: [anAssistant({ tools: ["Read", "Grep"], replaces: ["Grep"] })],
  });

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 900);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  // Seeded inside `cy.then` so it runs after the queued clears above.
  cy.then(() => window.localStorage.setItem("tddy_session_token", "fake-token"));
});

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

describe("AssistantTakeoverAcceptance — the main-agent tools an assistant stands in for", () => {
  it("asks what the assistant may call and what it takes over as two separate questions", () => {
    // Given
    mount(aRegistryWithOneModel());

    // When
    page.openCreateAssistant(QWEN);

    // Then — two pickers, each naming which question it answers; one list could not tell an
    // operator which of the two they were editing
    page.createAssistantTools().should("contain.text", "Tools");
    page.createAssistantReplacedTools().should("contain.text", "Replaces");
  });

  it("sends the takeover the operator picked alongside the tools the assistant may call", () => {
    // Given
    const backend = aRegistryWithOneModel();
    mount(backend);

    // When — an assistant that reads and greps for itself, and takes the main agent's search off it
    page.openCreateAssistant(QWEN);
    page.fillAndSubmitCreateAssistantForm({
      name: "repo-reader",
      label: "Repo Reader",
      systemPrompt: "You read code and answer questions about it.",
      tools: ["Read", "Grep"],
      replaces: ["Grep", "Glob"],
    });

    // Then — the two sets reach the daemon apart, each in its catalog order
    cy.wrap(backend).should((b) => {
      expect(recordedFields(b.callsTo(ModelRegistryService.method.createAssistant))).to.deep.equal([
        {
          sessionToken: "fake-token",
          name: "repo-reader",
          label: "Repo Reader",
          providerId: "prov-ollama",
          modelId: "qwen3:32b",
          systemPrompt: "You read code and answer questions about it.",
          tools: ["Read", "Grep"],
          replaces: ["Grep", "Glob"],
        },
      ]);
    });
  });

  it("takes nothing over from the main agent when no takeover box is ticked", () => {
    // Given
    const backend = aRegistryWithOneModel();
    mount(backend);

    // When — tools are picked and the takeover picker is left alone
    page.openCreateAssistant(QWEN);
    page.fillAndSubmitCreateAssistantForm({
      name: "repo-reader",
      label: "Repo Reader",
      systemPrompt: "You read code and answer questions about it.",
      tools: ["Read", "Grep"],
      replaces: [],
    });

    // Then — an untouched picker means an untouched main agent; ticking Grep for the assistant's
    // own loop must not withdraw Grep from the session that attaches it
    cy.wrap(backend).should((b) => {
      const [request] = recordedFields(b.callsTo(ModelRegistryService.method.createAssistant));
      expect(request.replaces).to.deep.equal([]);
      expect(request.tools).to.deep.equal(["Read", "Grep"]);
    });
  });

  it("sends the takeover in the daemon's catalog order rather than the order it was ticked", () => {
    // Given
    const backend = aRegistryWithOneModel();
    mount(backend);

    // When — ticked back to front
    page.openCreateAssistant(QWEN);
    page.fillAndSubmitCreateAssistantForm({
      name: "repo-reader",
      label: "Repo Reader",
      systemPrompt: "You read code and answer questions about it.",
      tools: ["Read"],
      replaces: ["Glob", "Grep"],
    });

    // Then — what is stored does not depend on the order the operator happened to click in
    cy.wrap(backend).should((b) => {
      const [request] = recordedFields(b.callsTo(ModelRegistryService.method.createAssistant));
      expect(request.replaces).to.deep.equal(["Grep", "Glob"]);
    });
  });

  it("opens the edit dialog on the takeover the daemon holds", () => {
    // Given an assistant that already takes Grep over
    mount(aRegistryWithAGrepTakeover());

    // When
    page.openEditAssistant(REPO_READER);

    // Then the dialog shows the stored takeover, so saving without touching it cannot hand Grep
    // silently back to the main agent
    page.editAssistantReplacedToolBox("Grep").should("be.checked");
    page.editAssistantReplacedToolBox("Glob").should("not.be.checked");
  });

  it("saves a widened takeover to the owning daemon", () => {
    // Given the same assistant, to be widened from Grep to the whole search surface
    const backend = aRegistryWithAGrepTakeover();
    mount(backend);

    // When
    page.openEditAssistant(REPO_READER);
    page.fillAndSubmitEditAssistantForm({
      label: "Repo Reader",
      systemPrompt: "You read code and answer questions about it.",
      tools: ["Read", "Grep"],
      replaces: ["Grep", "Glob"],
    });

    // Then the update carries the whole takeover, exactly as the dialog showed it
    cy.wrap(backend).should((b) => {
      expect(recordedFields(b.callsTo(ModelRegistryService.method.updateAssistant))).to.deep.equal([
        {
          sessionToken: "fake-token",
          assistantId: "asst-1",
          label: "Repo Reader",
          systemPrompt: "You read code and answer questions about it.",
          tools: ["Read", "Grep"],
          replaces: ["Grep", "Glob"],
        },
      ]);
    });
  });

  it("saves a takeover the operator gave up, rather than keeping the one it was opened on", () => {
    // Given the same assistant, whose takeover the operator unticks
    const backend = aRegistryWithAGrepTakeover();
    mount(backend);

    // When
    page.openEditAssistant(REPO_READER);
    page.fillAndSubmitEditAssistantForm({
      label: "Repo Reader",
      systemPrompt: "You read code and answer questions about it.",
      tools: ["Read", "Grep"],
      replaces: [],
    });

    // Then — an emptied picker is an emptied takeover; an update that omitted it would leave the
    // main agent's Grep withdrawn forever with nothing on screen saying so
    cy.wrap(backend).should((b) => {
      const [request] = recordedFields(b.callsTo(ModelRegistryService.method.updateAssistant));
      expect(request.replaces).to.deep.equal([]);
      expect(request.tools).to.deep.equal(["Read", "Grep"]);
    });
  });
});
