/**
 * Acceptance: an assistant is composed from a model plus a system prompt plus a selection of tools,
 * the tool choices come from the daemon's exec catalog (the web holds no tool list of its own), and
 * the created assistant is listed with the tools it was given.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md (AC8).
 */

import React from "react";
import { Code } from "@connectrpc/connect";
import { create } from "@bufbuild/protobuf";
import {
  CreateAssistantResponseSchema,
  ModelRegistryService,
} from "../../../src/gen/models_pb";
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
/** The name the daemon rejects, so no row for it may ever appear. */
const CURSOR: AssistantRef = { daemonInstanceId: FIXTURE_DAEMON, name: "cursor" };

const mount = (backend: ReturnType<typeof aModelRegistryBackend>) =>
  mountWithRpc(
    withSelectedDaemon(<ModelsAppPage onNavigate={cy.stub()} />, [FIXTURE_HOST]),
    backend,
  );

const aRegistryWithOneModel = () =>
  aModelRegistryBackend({ providers: [anOllamaProvider()], models: [anLlmModel()] });

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

describe("AssistantsPanelAcceptance — composing a model and tools into an assistant", () => {
  it("offers exactly the exec-catalog tools the daemon advertises", () => {
    // Given
    mount(aRegistryWithOneModel());

    // When
    page.openCreateAssistant(QWEN);

    // Then — the daemon's catalog, in the daemon's order; nothing added by the web
    page
      .assignableToolNames()
      .should("deep.equal", [
        "Read",
        "Write",
        "StrReplace",
        "Delete",
        "Grep",
        "Glob",
        "Shell",
        "Await",
        "ReadLints",
        "SemanticSearch",
      ]);
  });

  it("creates an assistant from a model with the selected tools", () => {
    // Given
    const backend = aRegistryWithOneModel();
    mount(backend);

    // When
    page.openCreateAssistant(QWEN);
    page.fillAndSubmitCreateAssistantForm({
      name: "repo-reader",
      label: "Repo Reader",
      systemPrompt: "You read code and answer questions about it.",
      tools: ["Read", "Grep"],
      replaces: [],
    });

    // Then — the model and provider of the originating row are carried into the assistant
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
          replaces: [],
        },
      ]);
    });
    page.assistantRow(REPO_READER).should("contain.text", "Repo Reader");
    page.assistantTools(REPO_READER).should("deep.equal", ["Read", "Grep"]);
  });

  it("reports a name that collides with an existing agent instead of listing a second assistant", () => {
    // Given — the daemon rejects the name, as it would for a builtin agent id
    const backend = aModelRegistryBackend({
      providers: [anOllamaProvider()],
      models: [anLlmModel()],
      assistants: [anAssistant()],
    }).failWith(
      ModelRegistryService.method.createAssistant,
      Code.AlreadyExists,
      "agent name 'cursor' is already taken",
    );
    mount(backend);

    // When
    page.openCreateAssistant(QWEN);
    page.fillAndSubmitCreateAssistantForm({
      name: "cursor",
      label: "Cursor",
      systemPrompt: "You answer questions about the repository.",
      tools: ["Read"],
      replaces: [],
    });

    // Then — the dialog assertion waits for the rejection to have been rendered, so the
    // `timeout: 0` absence below runs after the create round trip has already come back
    page.createAssistantDialog().should("contain.text", "agent name 'cursor' is already taken");
    page.assistantRow(CURSOR, { timeout: 0 }).should("not.exist");
  });

  it("creates one assistant when the operator submits twice before the daemon answers", () => {
    // Given — a daemon whose create only answers once the test releases it
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const backend = aRegistryWithOneModel().onUnary(
      ModelRegistryService.method.createAssistant,
      async () => {
        await gate;
        return create(CreateAssistantResponseSchema, { assistant: anAssistant() });
      },
    );
    mount(backend);

    // When — the operator submits, then submits again while the first create is still in flight
    page.openCreateAssistant(QWEN);
    page.fillCreateAssistantForm({
      name: "repo-reader",
      label: "Repo Reader",
      systemPrompt: "You read code and answer questions about it.",
      tools: ["Read"],
      replaces: [],
    });
    page.createAssistantSubmit().click();
    page.createAssistantSubmit().should("be.disabled");
    page.createAssistantSubmit().click({ force: true });
    cy.then(() => release());

    // Then — one assistant, not the two an impatient operator would otherwise mint
    cy.wrap(backend).should((b) => {
      expect(b.callsTo(ModelRegistryService.method.createAssistant)).to.have.length(1);
    });
  });

  it("reports that the daemon's tool catalog could not be read instead of offering no tools", () => {
    // Given — a daemon that serves its models but not its exec catalog
    const backend = aRegistryWithOneModel().failWith(
      ModelRegistryService.method.listAssignableTools,
      Code.Internal,
      "tool catalog unavailable: engine not initialised",
    );
    mount(backend);

    // When
    page.openCreateAssistant(QWEN);

    // Then — the dialog says the catalog is unknown, in the daemon's own words; an empty tool list
    // would read as "this daemon assigns no tools"
    page.createAssistantToolCatalogStatus().should("equal", "unavailable");
    page
      .createAssistantTools()
      .should("contain.text", "tool catalog unavailable: engine not initialised");
  });

  it("refuses to create an assistant while the daemon's tool catalog is unknown", () => {
    // Given — the same daemon, whose exec catalog cannot be read
    const backend = aRegistryWithOneModel().failWith(
      ModelRegistryService.method.listAssignableTools,
      Code.Internal,
      "tool catalog unavailable: engine not initialised",
    );
    mount(backend);

    // When
    page.openCreateAssistant(QWEN);
    page.fillCreateAssistantForm({
      name: "repo-reader",
      label: "Repo Reader",
      systemPrompt: "You read code and answer questions about it.",
      tools: [],
      replaces: [],
    });

    // Then — an assistant is stored with the tools it was created with, so one composed from a
    // catalog that never arrived would be a permanently toolless agent nobody meant to define
    page.createAssistantSubmit().should("be.disabled");
  });
});
