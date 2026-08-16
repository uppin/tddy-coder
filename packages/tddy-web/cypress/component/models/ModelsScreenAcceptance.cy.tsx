/**
 * Acceptance: the Models & Agents screen renders every model the fleet's providers offer, labelled
 * by capability and by residency, with the actions each model's state actually permits.
 *
 * These specs exercise a single daemon; the cross-daemon merge and owning-daemon routing live in
 * `ModelsCrossHostAcceptance.cy.tsx`.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md (AC2, AC3, AC4, AC11).
 */

import React from "react";
import { Code } from "@connectrpc/connect";
import { ModelLoadState, ModelRegistryService } from "../../../src/gen/models_pb";
import { ModelsAppPage } from "../../../src/components/models/ModelsAppPage";
import type { DaemonHost } from "../../../src/lib/participantRole";
import { withSelectedDaemon } from "../../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../../support/rpc/inMemory";
import {
  aCloudModel,
  aCloudProvider,
  aModelRegistryBackend,
  anEmbeddingModel,
  anLlmModel,
  anOllamaProvider,
  FIXTURE_DAEMON,
} from "../../support/rpc/modelRegistryBackend";
import { modelsScreenPage as page, type ModelRef } from "../../support/pages/modelsScreenPage";
import { recordedFields } from "../../support/rpc/recordedRequests";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/** The single daemon these specs run against — its id matches the fixture stamps. */
const FIXTURE_HOST: DaemonHost = {
  instanceId: FIXTURE_DAEMON,
  label: `${FIXTURE_DAEMON} (this daemon)`,
};

const QWEN: ModelRef = {
  daemonInstanceId: FIXTURE_DAEMON,
  providerId: "prov-ollama",
  modelId: "qwen3:32b",
};
const NOMIC: ModelRef = {
  daemonInstanceId: FIXTURE_DAEMON,
  providerId: "prov-ollama",
  modelId: "nomic-embed-text",
};
const KIMI: ModelRef = {
  daemonInstanceId: FIXTURE_DAEMON,
  providerId: "prov-fireworks",
  modelId: "accounts/fireworks/models/kimi-k2",
};
/** A cloud model whose capabilities the provider's metadata did not reveal. */
const UNKNOWN: ModelRef = {
  daemonInstanceId: FIXTURE_DAEMON,
  providerId: "prov-fireworks",
  modelId: "accounts/fireworks/models/text-embedding-3",
};

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

describe("ModelsScreenAcceptance — the model catalog", () => {
  it("labels an embedding model as embedding and offers it no chat action", () => {
    // Given — one chat-capable model and one embedding model on the same provider
    const backend = aModelRegistryBackend({
      providers: [anOllamaProvider()],
      models: [anLlmModel(), anEmbeddingModel()],
    });

    // When
    mountWithRpc(withSelectedDaemon(<ModelsAppPage onNavigate={cy.stub()} />, [FIXTURE_HOST]), backend);

    // Then — `rowLabels` above waits for the embedding row to render, so the `timeout: 0`
    // absence below is read from a table that is already populated, not from an empty page
    page.rowLabels(NOMIC).should("deep.equal", ["embedding"]);
    page.chatButton(NOMIC, { timeout: 0 }).should("not.exist");
    page.chatButton(QWEN).should("exist");
  });

  it("offers Unload for a resident model and Load for a model that is not resident", () => {
    // Given — the same provider serving one resident and one evicted model
    const backend = aModelRegistryBackend({
      providers: [anOllamaProvider()],
      models: [
        anLlmModel({ loadState: ModelLoadState.LOADED }),
        anEmbeddingModel({ loadState: ModelLoadState.NOT_LOADED }),
      ],
    });

    // When
    mountWithRpc(withSelectedDaemon(<ModelsAppPage onNavigate={cy.stub()} />, [FIXTURE_HOST]), backend);

    // Then — each `timeout: 0` absence is preceded by a waiting assertion on the *same* row
    // (`rowLoadState`, then the action that row does offer), so the row is known to be rendered
    page.rowLoadState(QWEN).should("equal", "loaded");
    page.unloadButton(QWEN).should("exist");
    page.loadButton(QWEN, { timeout: 0 }).should("not.exist");

    page.rowLoadState(NOMIC).should("equal", "not_loaded");
    page.loadButton(NOMIC).should("exist");
    page.unloadButton(NOMIC, { timeout: 0 }).should("not.exist");
  });

  it("marks a model as resident after loading it", () => {
    // Given
    const backend = aModelRegistryBackend({
      providers: [anOllamaProvider()],
      models: [anLlmModel({ loadState: ModelLoadState.NOT_LOADED })],
    });
    mountWithRpc(withSelectedDaemon(<ModelsAppPage onNavigate={cy.stub()} />, [FIXTURE_HOST]), backend);

    // When
    page.loadModel(QWEN);

    // Then
    cy.wrap(backend).should((b) => {
      expect(recordedFields(b.callsTo(ModelRegistryService.method.loadModel))).to.deep.equal([
        { sessionToken: "fake-token", providerId: "prov-ollama", modelId: "qwen3:32b" },
      ]);
    });
    page.rowLoadState(QWEN).should("equal", "loaded");
  });

  it("marks a model as not resident after unloading it", () => {
    // Given — a resident model
    const backend = aModelRegistryBackend({
      providers: [anOllamaProvider()],
      models: [anLlmModel({ loadState: ModelLoadState.LOADED })],
    });
    mountWithRpc(withSelectedDaemon(<ModelsAppPage onNavigate={cy.stub()} />, [FIXTURE_HOST]), backend);

    // When
    page.unloadModel(QWEN);

    // Then
    cy.wrap(backend).should((b) => {
      expect(recordedFields(b.callsTo(ModelRegistryService.method.unloadModel))).to.deep.equal([
        { sessionToken: "fake-token", providerId: "prov-ollama", modelId: "qwen3:32b" },
      ]);
    });
    page.rowLoadState(QWEN).should("equal", "not_loaded");
  });

  it("reports on the row why the owning daemon refused to load a model", () => {
    // Given — a daemon that cannot load the model and says why
    const backend = aModelRegistryBackend({
      providers: [anOllamaProvider()],
      models: [anLlmModel({ loadState: ModelLoadState.NOT_LOADED })],
    }).failWith(
      ModelRegistryService.method.loadModel,
      Code.Unavailable,
      "connection refused: http://localhost:11434/api/generate",
    );
    mountWithRpc(withSelectedDaemon(<ModelsAppPage onNavigate={cy.stub()} />, [FIXTURE_HOST]), backend);

    // When
    page.loadModel(QWEN);

    // Then — the daemon's own words, against the row the operator acted on, and no residency
    // the daemon never granted
    page
      .rowError(QWEN)
      .should("contain.text", "connection refused: http://localhost:11434/api/generate");
    page.rowLoadState(QWEN).should("equal", "not_loaded");
  });

  it("offers no chat for a model whose capabilities the daemon could not determine", () => {
    // Given — a cloud model the daemon could only label `unknown`, alongside one it labelled `llm`
    const backend = aModelRegistryBackend({
      providers: [anOllamaProvider(), aCloudProvider()],
      models: [
        anLlmModel(),
        aCloudModel({ modelId: UNKNOWN.modelId, label: "Text Embedding 3", labels: ["unknown"] }),
      ],
    });

    // When
    mountWithRpc(withSelectedDaemon(<ModelsAppPage onNavigate={cy.stub()} />, [FIXTURE_HOST]), backend);

    // Then — chat needs a positive `llm` label; `unknown` is the daemon refusing to guess, and the
    // web must not guess in its place. `rowLabels` waits for the row, so the absence below is read
    // from a table that has already rendered it
    page.rowLabels(UNKNOWN).should("deep.equal", ["unknown"]);
    page.chatButton(UNKNOWN, { timeout: 0 }).should("not.exist");
    page.chatButton(QWEN).should("exist");
  });

  it("keeps a daemon's models listed when only its assistant list fails to read", () => {
    // Given — a daemon that answers for providers, models and tools, but not for assistants
    const backend = aModelRegistryBackend({
      providers: [anOllamaProvider()],
      models: [anLlmModel()],
    }).failWith(
      ModelRegistryService.method.listAssistants,
      Code.Internal,
      "no SubagentTool variant for tool 'Sleep'",
    );

    // When
    mountWithRpc(withSelectedDaemon(<ModelsAppPage onNavigate={cy.stub()} />, [FIXTURE_HOST]), backend);

    // Then — the lost list is named and the lists that arrived still render
    page
      .daemonError(FIXTURE_DAEMON)
      .should("contain.text", "assistants: no SubagentTool variant for tool 'Sleep'");
    page.row(QWEN).should("exist");
    page.providerRow({ daemonInstanceId: FIXTURE_DAEMON, providerId: "prov-ollama" }).should("exist");
  });

  it("renders a cloud model as residency-unsupported and offers neither Load nor Unload", () => {
    // Given — a cloud model, for which residency has no meaning
    const backend = aModelRegistryBackend({
      providers: [anOllamaProvider(), aCloudProvider()],
      models: [anLlmModel(), aCloudModel()],
    });
    mountWithRpc(withSelectedDaemon(<ModelsAppPage onNavigate={cy.stub()} />, [FIXTURE_HOST]), backend);

    // Then — the row states residency does not apply and offers neither action. `rowLoadState`
    // waits for the cloud row itself, so both `timeout: 0` absences run against a rendered row
    page.rowLoadState(KIMI).should("equal", "unsupported");
    page.loadButton(KIMI, { timeout: 0 }).should("not.exist");
    page.unloadButton(KIMI, { timeout: 0 }).should("not.exist");
  });
});
