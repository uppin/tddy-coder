/**
 * Acceptance: providers are added explicitly through the screen (never auto-detected), the API key
 * travels inbound only, and a provider whose enumeration fails says so instead of showing a stale
 * or invented model list.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md (AC6, AC7).
 */

import React from "react";
import { Code } from "@connectrpc/connect";
import { ModelRegistryService, ProviderKind } from "../../../src/gen/models_pb";
import { ModelsAppPage } from "../../../src/components/models/ModelsAppPage";
import type { DaemonHost } from "../../../src/lib/participantRole";
import { withSelectedDaemon } from "../../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../../support/rpc/inMemory";
import {
  aCloudModel,
  aCloudProvider,
  aModelRegistryBackend,
  anLlmModel,
  anOllamaProvider,
  FIXTURE_DAEMON,
} from "../../support/rpc/modelRegistryBackend";
import {
  modelsScreenPage as page,
  type ModelRef,
  type ProviderRef,
} from "../../support/pages/modelsScreenPage";
import { recordedFields } from "../../support/rpc/recordedRequests";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const FIXTURE_HOST: DaemonHost = {
  instanceId: FIXTURE_DAEMON,
  label: `${FIXTURE_DAEMON} (this daemon)`,
};

const OLLAMA: ProviderRef = { daemonInstanceId: FIXTURE_DAEMON, providerId: "prov-ollama" };
/** The provider the daemon mints for a newly created one. */
const CREATED: ProviderRef = { daemonInstanceId: FIXTURE_DAEMON, providerId: "prov-1" };

const QWEN: ModelRef = {
  daemonInstanceId: FIXTURE_DAEMON,
  providerId: "prov-ollama",
  modelId: "qwen3:32b",
};
/** A model on a second, healthy provider — the control the failing provider is read against. */
const KIMI: ModelRef = {
  daemonInstanceId: FIXTURE_DAEMON,
  providerId: "prov-fireworks",
  modelId: "accounts/fireworks/models/kimi-k2",
};

const mount = (backend: ReturnType<typeof aModelRegistryBackend>) =>
  mountWithRpc(
    withSelectedDaemon(<ModelsAppPage onNavigate={cy.stub()} />, [FIXTURE_HOST]),
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

describe("ProvidersPanelAcceptance — adding and reporting providers", () => {
  it("adds a cloud provider from the form and lists it as holding a credential", () => {
    // Given — a daemon with no providers configured yet
    const backend = aModelRegistryBackend({ providers: [], models: [] });
    mount(backend);

    // When
    page.openAddProviderForm();
    page.fillAndSubmitAddProviderForm({
      kind: String(ProviderKind.FIREWORKS),
      label: "Fireworks",
      baseUrl: "https://api.fireworks.ai/inference",
      apiKey: "fw-secret-key",
    });

    // Then — the key was carried by the create call and by nothing else
    cy.wrap(backend).should((b) => {
      expect(recordedFields(b.callsTo(ModelRegistryService.method.createProvider))).to.deep.equal([
        {
          sessionToken: "fake-token",
          kind: ProviderKind.FIREWORKS,
          label: "Fireworks",
          baseUrl: "https://api.fireworks.ai/inference",
          apiKey: "fw-secret-key",
        },
      ]);
    });
    page.providerRow(CREATED).should("contain.text", "Fireworks");
    page.providerHasCredential(CREATED).should("equal", "true");
  });

  it("reports a failing provider's enumeration error and lists none of its models", () => {
    // Given — two providers: one whose last refresh failed, and one healthy one serving a model,
    // so an empty table cannot be what makes the claim below true
    const backend = aModelRegistryBackend({
      providers: [
        anOllamaProvider({
          enumerationError: "connection refused: http://localhost:11434/api/tags",
        }),
        aCloudProvider(),
      ],
      models: [aCloudModel()],
    });

    // When
    mount(backend);

    // Then — the failure is visible, the healthy provider's model still lists, and no model is
    // invented for the failing one. `row(KIMI)` waits for the populated table, so the `timeout: 0`
    // absence that follows is read from a table that has already rendered
    page
      .providerError(OLLAMA)
      .should("contain.text", "connection refused: http://localhost:11434/api/tags");
    page.row(KIMI).should("exist");
    page.row(QWEN, { timeout: 0 }).should("not.exist");
  });

  it("marks a model as stale when its provider's last enumeration failed", () => {
    // Given — a provider whose enumeration failed, still holding the catalog it enumerated before
    const backend = aModelRegistryBackend({
      providers: [
        anOllamaProvider({
          enumerationError: "connection refused: http://localhost:11434/api/tags",
        }),
        aCloudProvider(),
      ],
      models: [anLlmModel(), aCloudModel()],
    });

    // When
    mount(backend);

    // Then — the row is still shown (the daemon really did serve it once) but it is not passed off
    // as current, which is what an unmarked row in a table headed "Models" would claim
    page.rowIsStale(QWEN).should("equal", "true");
    page.rowStaleMarker(QWEN).should("contain.text", "Stale");
  });

  it("leaves a healthy provider's models unmarked while another provider is failing", () => {
    // Given — the same two providers, one failing and one healthy
    const backend = aModelRegistryBackend({
      providers: [
        anOllamaProvider({
          enumerationError: "connection refused: http://localhost:11434/api/tags",
        }),
        aCloudProvider(),
      ],
      models: [anLlmModel(), aCloudModel()],
    });

    // When
    mount(backend);

    // Then — staleness follows the provider that failed, not the table
    page.rowIsStale(KIMI).should("equal", "false");
  });

  it("marks the catalog kept from before a failed refresh as stale", () => {
    // Given — a provider that lists one model, whose next refresh fails
    const backend = aModelRegistryBackend({
      providers: [anOllamaProvider()],
      models: [anLlmModel()],
    }).failWith(
      ModelRegistryService.method.refreshProviderModels,
      Code.Unavailable,
      "connection refused: http://localhost:11434/api/tags",
    );
    mount(backend);
    page.rowIsStale(QWEN).should("equal", "false");

    // When
    page.refreshProvider(OLLAMA);

    // Then — the daemon left its cache untouched and still serves the row, so the row itself has to
    // say the catalog behind it could not be confirmed (AC7)
    page.rowIsStale(QWEN).should("equal", "true");
  });

  it("keeps a provider's stale catalog on screen but marks the provider as failed to refresh", () => {
    // Given — a provider that lists one model, whose next refresh fails
    const backend = aModelRegistryBackend({
      providers: [anOllamaProvider()],
      models: [anLlmModel()],
    }).failWith(
      ModelRegistryService.method.refreshProviderModels,
      Code.Unavailable,
      "connection refused: http://localhost:11434/api/tags",
    );
    mount(backend);

    // When
    page.refreshProvider(OLLAMA);

    // Then — the row from the last successful enumeration is kept (dropping it would claim the
    // host has no models, which is not what happened), and the provider now says why it is stale
    page
      .providerError(OLLAMA)
      .should("contain.text", "connection refused: http://localhost:11434/api/tags");
    page.row(QWEN).should("exist");
  });
});
