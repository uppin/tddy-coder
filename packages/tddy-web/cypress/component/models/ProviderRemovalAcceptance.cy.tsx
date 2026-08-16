/**
 * Acceptance: a provider can be removed from the daemon that owns it, and a removal the daemon
 * refuses says so against that provider's row.
 *
 * Removal is not cosmetic here. `CreateProvider` rejects a base URL that is already configured and a
 * provider id is never re-used, so a mistyped provider that cannot be deleted occupies its base URL
 * on that daemon for good.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md (AC6).
 */

import React from "react";
import { Code } from "@connectrpc/connect";
import { ModelRegistryService } from "../../../src/gen/models_pb";
import { ModelsAppPage } from "../../../src/components/models/ModelsAppPage";
import type { DaemonHost } from "../../../src/lib/participantRole";
import { withSelectedDaemon } from "../../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../../support/rpc/inMemory";
import {
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
/** A second provider, so an emptied panel cannot be what makes a removal look successful. */
const FIREWORKS: ProviderRef = { daemonInstanceId: FIXTURE_DAEMON, providerId: "prov-fireworks" };

const QWEN: ModelRef = {
  daemonInstanceId: FIXTURE_DAEMON,
  providerId: "prov-ollama",
  modelId: "qwen3:32b",
};

const mount = (backend: ReturnType<typeof aModelRegistryBackend>) =>
  mountWithRpc(
    withSelectedDaemon(<ModelsAppPage onNavigate={cy.stub()} />, [FIXTURE_HOST]),
    backend,
  );

const aRegistryWithTwoProviders = () =>
  aModelRegistryBackend({
    providers: [anOllamaProvider(), aCloudProvider()],
    models: [anLlmModel()],
  });

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

describe("ProviderRemovalAcceptance — removing a provider from its daemon", () => {
  it("removes a provider and the models it served", () => {
    // Given — two providers, one of them serving a model
    const backend = aRegistryWithTwoProviders();
    mount(backend);

    // When
    page.deleteProvider(OLLAMA);

    // Then — the daemon was asked to delete that provider, and the re-read registry no longer
    // carries it or its model, while the other provider is untouched
    cy.wrap(backend).should((b) => {
      expect(recordedFields(b.callsTo(ModelRegistryService.method.deleteProvider))).to.deep.equal([
        { sessionToken: "fake-token", providerId: "prov-ollama" },
      ]);
    });
    page.providerRow(FIREWORKS).should("exist");
    page.providerRow(OLLAMA, { timeout: 0 }).should("not.exist");
    page.row(QWEN, { timeout: 0 }).should("not.exist");
  });

  it("reports a removal the daemon refused as permission denied, keeping the provider listed", () => {
    // Given — a daemon that only serves writes for the host that owns the row
    const backend = aRegistryWithTwoProviders().failWith(
      ModelRegistryService.method.deleteProvider,
      Code.PermissionDenied,
      "provider prov-ollama is owned by workstation-1",
    );
    mount(backend);

    // When
    page.deleteProvider(OLLAMA);

    // Then — the refusal is named as a refusal, in the daemon's own words, and the provider is
    // still there, because it is
    page
      .providerActionError(OLLAMA)
      .should("have.text", "Permission denied — provider prov-ollama is owned by workstation-1");
    page.providerRow(OLLAMA).should("exist");
  });

  it("reports a removal blocked by an assistant without marking the provider's models stale", () => {
    // Given — a daemon that refuses to orphan an assistant
    const backend = aRegistryWithTwoProviders().failWith(
      ModelRegistryService.method.deleteProvider,
      Code.FailedPrecondition,
      "assistant 'repo-reader' still references this provider",
    );
    mount(backend);

    // When
    page.deleteProvider(OLLAMA);

    // Then — a refused write says nothing about how fresh the catalog is, so the provider's models
    // must not be marked as never having been enumerated
    page
      .providerActionError(OLLAMA)
      .should("have.text", "assistant 'repo-reader' still references this provider");
    page.rowIsStale(QWEN).should("equal", "false");
  });
});
