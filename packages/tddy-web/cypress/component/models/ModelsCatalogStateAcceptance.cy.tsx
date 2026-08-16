/**
 * Acceptance: an empty Models & Agents table always says *why* it is empty.
 *
 * A room that never connected, a room with no daemons in it, a read still in flight and a fleet
 * that genuinely has no models all render the same blank table otherwise — and the first three are
 * not "there are no models", which is what a blank table under that heading claims.
 *
 * The same rule covers adding a provider with no daemon selected: a provider belongs to exactly one
 * daemon, so the form has to say there is none rather than address the empty instance id.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md (§ Non-Functional Requirements —
 * nothing degrades silently).
 */

import React from "react";
import { Room } from "livekit-client";
import { create } from "@bufbuild/protobuf";
import { type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  ListModelsResponseSchema,
  ModelRegistryService,
  ProviderKind,
} from "../../../src/gen/models_pb";
import { ModelsAppPage } from "../../../src/components/models/ModelsAppPage";
import { AuthProvider } from "../../../src/hooks/authProvider";
import type { DaemonHost } from "../../../src/lib/participantRole";
import { SelectedDaemonProvider } from "../../../src/rpc/selectedDaemon";
import { mountWithRpc } from "../../support/rpc/inMemory";
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

/**
 * Mount the screen with an explicit common-room connection: `null` stands for a room that is not
 * connected, which is exactly what `SelectedDaemonProvider` publishes before (and after) a join.
 */
function mountWithRoom(
  backend: InMemoryRpcBackend,
  options: { room: Room | null; daemons: DaemonHost[] },
) {
  return mountWithRpc(
    <AuthProvider>
      <SelectedDaemonProvider room={options.room} daemons={options.daemons}>
        <ModelsAppPage onNavigate={cy.stub()} />
      </SelectedDaemonProvider>
    </AuthProvider>,
    backend,
  );
}

/** A registry whose model list only answers once the test releases it. */
function aGatedRegistryBackend(gate: Promise<void>): InMemoryRpcBackend {
  return aModelRegistryBackend({
    providers: [anOllamaProvider()],
    models: [anLlmModel()],
  }).onUnary(ModelRegistryService.method.listModels, async () => {
    await gate;
    return create(ListModelsResponseSchema, { models: [anLlmModel()] });
  });
}

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

describe("ModelsCatalogStateAcceptance — why the catalog is empty", () => {
  it("reports that the common room is not connected rather than an empty catalog", () => {
    // Given — no common-room connection, so no daemon can be addressed at all
    // When
    mountWithRoom(aModelRegistryBackend({ providers: [], models: [] }), {
      room: null,
      daemons: [],
    });

    // Then
    page.emptyStateStatus().should("equal", "not-connected");
  });

  it("reports a daemon it cannot address rather than listing none of its models", () => {
    // Given — a daemon is known, but the room that would carry RPC to it is not connected
    // When
    mountWithRoom(aModelRegistryBackend({ providers: [], models: [] }), {
      room: null,
      daemons: [FIXTURE_HOST],
    });

    // Then — the daemon is named, in the same words every other action uses
    page
      .daemonError(FIXTURE_DAEMON)
      .should("contain.text", `no connection to daemon ${FIXTURE_DAEMON}`);
  });

  it("reports an empty common room rather than an empty catalog", () => {
    // Given — connected, but no daemon has joined
    // When
    mountWithRoom(aModelRegistryBackend({ providers: [], models: [] }), {
      room: new Room(),
      daemons: [],
    });

    // Then
    page.emptyStateStatus().should("equal", "no-daemons");
  });

  it("reports the catalog as still being read until the daemon answers", () => {
    // Given — a daemon whose model list has not come back yet
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    mountWithRoom(aGatedRegistryBackend(gate), { room: new Room(), daemons: [FIXTURE_HOST] });

    // Then — the wait is stated, not rendered as "this fleet has no models"
    page.emptyStateStatus().should("equal", "loading");

    // When — the daemon answers
    cy.then(() => release());

    // Then
    page.row(QWEN).should("exist");
  });

  it("reports a daemon that answered with no models as an empty catalog", () => {
    // Given — a daemon with a provider that offers nothing
    // When
    mountWithRoom(aModelRegistryBackend({ providers: [anOllamaProvider()], models: [] }), {
      room: new Room(),
      daemons: [FIXTURE_HOST],
    });

    // Then
    page.emptyStateStatus().should("equal", "ready");
  });

  it("refuses to add a provider while no daemon is selected", () => {
    // Given — connected, but nothing to attach a provider to
    const backend = aModelRegistryBackend({ providers: [], models: [] });
    mountWithRoom(backend, { room: new Room(), daemons: [] });

    // When
    page.openAddProviderForm();
    page.fillAndSubmitAddProviderForm({
      kind: String(ProviderKind.OLLAMA),
      label: "Local Ollama",
      baseUrl: "http://localhost:11434",
      apiKey: "unused-key",
    });

    // Then — the operator is told what is missing, and nothing was sent to a nameless daemon
    page
      .addProviderError()
      .should("have.text", "select a daemon before adding a provider");
    cy.wrap(backend).should((b) => {
      expect(b.callsTo(ModelRegistryService.method.createProvider)).to.have.length(0);
    });
  });
});
