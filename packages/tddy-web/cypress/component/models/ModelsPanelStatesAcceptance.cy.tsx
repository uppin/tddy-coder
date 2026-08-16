/**
 * Acceptance: the providers and assistants panels always say *why* they hold no rows.
 *
 * A first-run daemon with nothing configured, a read still in flight, a room that never connected
 * and a `ListProviders` that failed are the same empty panel otherwise — and only the first is the
 * claim "this fleet has none", which is what a heading over an empty panel makes.
 *
 * The models table already answers this question (`ModelsCatalogStateAcceptance`); these are the
 * two panels beside it.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md (§ Non-Functional Requirements —
 * nothing degrades silently).
 */

import React from "react";
import { Code } from "@connectrpc/connect";
import { Room } from "livekit-client";
import { type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { ModelRegistryService } from "../../../src/gen/models_pb";
import { ModelsAppPage } from "../../../src/components/models/ModelsAppPage";
import { AuthProvider } from "../../../src/hooks/authProvider";
import type { DaemonHost } from "../../../src/lib/participantRole";
import { SelectedDaemonProvider } from "../../../src/rpc/selectedDaemon";
import { mountWithRpc } from "../../support/rpc/inMemory";
import {
  aModelRegistryBackend,
  anAssistant,
  anLlmModel,
  anOllamaProvider,
  FIXTURE_DAEMON,
} from "../../support/rpc/modelRegistryBackend";
import { modelsScreenPage as page } from "../../support/pages/modelsScreenPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const FIXTURE_HOST: DaemonHost = {
  instanceId: FIXTURE_DAEMON,
  label: `${FIXTURE_DAEMON} (this daemon)`,
};

/** Mount with an explicit common-room connection; `null` is a room that is not connected. */
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

const aConnectedDaemon = (backend: InMemoryRpcBackend) =>
  mountWithRoom(backend, { room: new Room(), daemons: [FIXTURE_HOST] });

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

describe("ModelsPanelStatesAcceptance — why a panel holds no rows", () => {
  it("reports a daemon with nothing configured as having no providers", () => {
    // Given — the first-run case: a daemon that answered, with an empty provider list
    // When
    aConnectedDaemon(aModelRegistryBackend({ providers: [], models: [] }));

    // Then — the panel itself makes the claim, in words, and says which of the four read states
    // entitles it to make one
    page.providersEmptyStatus().should("equal", "ready");
    page.providersPanel().should("contain.text", "No providers configured");
  });

  it("reports the providers panel as not connected rather than as having no providers", () => {
    // Given — no common-room connection, so no daemon can be asked at all
    // When
    mountWithRoom(aModelRegistryBackend({ providers: [], models: [] }), {
      room: null,
      daemons: [],
    });

    // Then
    page.providersEmptyStatus().should("equal", "not-connected");
  });

  it("reports a failed provider read in the providers panel, not only in the models table", () => {
    // Given — a daemon that cannot serve its provider list
    // When
    aConnectedDaemon(
      aModelRegistryBackend({ providers: [], models: [] }).failWith(
        ModelRegistryService.method.listProviders,
        Code.Unavailable,
        "common room peer is unreachable",
      ),
    );

    // Then — the panel says the read failed, in the daemon's own words. Without it, "no providers
    // configured" would be the panel's answer to a question it never got an answer to
    page
      .providersDaemonError(FIXTURE_DAEMON)
      .should("contain.text", "providers: common room peer is unreachable");
    page.providersEmptyState({ timeout: 0 }).should("not.exist");
  });

  it("reports a daemon with no assistants defined as having none", () => {
    // Given — a daemon with a model but no assistant composed from it
    // When
    aConnectedDaemon(
      aModelRegistryBackend({
        providers: [anOllamaProvider()],
        models: [anLlmModel()],
        assistants: [],
      }),
    );

    // Then
    page.assistantsEmptyStatus().should("equal", "ready");
    page.assistantsPanel().should("contain.text", "No assistants defined");
  });

  it("reports a failed assistant read in the assistants panel rather than an empty panel", () => {
    // Given — a daemon that serves its models but not its assistants
    // When
    aConnectedDaemon(
      aModelRegistryBackend({
        providers: [anOllamaProvider()],
        models: [anLlmModel()],
        assistants: [anAssistant()],
      }).failWith(
        ModelRegistryService.method.listAssistants,
        Code.Internal,
        "no SubagentTool variant for tool 'Sleep'",
      ),
    );

    // Then
    page
      .assistantsDaemonError(FIXTURE_DAEMON)
      .should("contain.text", "assistants: no SubagentTool variant for tool 'Sleep'");
    page.assistantsEmptyState({ timeout: 0 }).should("not.exist");
  });
});
