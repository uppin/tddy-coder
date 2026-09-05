/**
 * Acceptance: how the Models & Agents fan-out behaves around the *connection*, rather than around
 * the data.
 *
 * Two properties with no surface of their own on the screen:
 *   • the daemon list is rebuilt on every common-room participant event, so the fan-out must key on
 *     the daemons themselves — re-reading four lists per daemon per re-render would put the fleet's
 *     registry on a treadmill;
 *   • an action taken while the room is down is answered, not swallowed. A Refresh that reports
 *     nothing reads as "refreshed, unchanged".
 *
 * Both are driven through small harnesses: the first re-renders the provider with a freshly built
 * daemon array, the second holds the hook while the room is `null`, which is a state the screen
 * itself cannot be clicked into (its provider rows are gone by then).
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md (§ Cross-daemon reads are web-side
 * fan-out).
 */

import React, { useMemo, useState } from "react";
import { Code, ConnectError } from "@connectrpc/connect";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  ListAssignableToolsResponseSchema,
  ListAssistantsResponseSchema,
  ListProvidersResponseSchema,
  ModelLoadState,
  ModelRegistryService,
  ProviderKind,
} from "../../../src/gen/models_pb";
import { ModelsAppPage } from "../../../src/components/models/ModelsAppPage";
import { useModelRegistryFanOut } from "../../../src/components/models/useModelRegistryFanOut";
import { AuthProvider } from "../../../src/hooks/authProvider";
import type { DaemonHost } from "../../../src/lib/participantRole";
import { SelectedDaemonProvider } from "../../../src/rpc/selectedDaemon";
import { providerRowKey, type ProviderRow } from "../../../src/utils/mergeRegistryEntries";
import { mountWithRpc } from "../../support/rpc/inMemory";
import { aJoinedCommonRoom } from "../../support/rpc/withSelectedDaemon";
import {
  aModelRegistryBackend,
  anLlmModel,
  anOllamaProvider,
  FIXTURE_DAEMON,
} from "../../support/rpc/modelRegistryBackend";
import { modelsScreenPage as page, type ModelRef } from "../../support/pages/modelsScreenPage";
import { byTestId } from "../../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const QWEN: ModelRef = {
  daemonInstanceId: FIXTURE_DAEMON,
  providerId: "prov-ollama",
  modelId: "qwen3:32b",
};

const OLLAMA: ProviderRow = {
  daemonInstanceId: FIXTURE_DAEMON,
  providerId: "prov-ollama",
  kind: ProviderKind.OLLAMA,
  label: "Local Ollama",
  baseUrl: "http://localhost:11434",
  hasCredential: false,
  enumerationError: "",
};

/** Test-owned controls of the harnesses below — not part of any screen. */
const HARNESS_IDS = {
  rebuildDaemons: "harness-rebuild-daemons",
  refresh: "harness-refresh-provider",
  refreshError: "harness-refresh-error",
  leaveScreen: "harness-leave-screen",
};

/**
 * A daemon whose model list never answers, and which records whether the read it is serving was
 * cancelled — the only evidence available for a read the screen has walked away from.
 */
function aRegistryWithANeverAnsweringModelList(): {
  backend: InMemoryRpcBackend;
  read: { cancelled: boolean };
} {
  const read = { cancelled: false };
  const backend = anInMemoryRpcBackend()
    .implement(ModelRegistryService, {
      listModels: async (_req, context) => {
        await new Promise<void>((resolve) => context.signal.addEventListener("abort", () => resolve()));
        read.cancelled = true;
        throw new ConnectError("the read was cancelled", Code.Canceled);
      },
    })
    .onUnary(ModelRegistryService.method.listProviders, () =>
      create(ListProvidersResponseSchema, { providers: [anOllamaProvider()] }),
    )
    .onUnary(ModelRegistryService.method.listAssistants, () =>
      create(ListAssistantsResponseSchema, { assistants: [] }),
    )
    .onUnary(ModelRegistryService.method.listAssignableTools, () =>
      create(ListAssignableToolsResponseSchema, { tools: [] }),
    );
  return { backend, read };
}

/**
 * Rebuilds the `daemons` array on every render — the shape `useRoomParticipants` produces, where a
 * participant event yields a new array holding the same daemons.
 */
function RebuiltDaemonListHarness() {
  const [rebuilds, setRebuilds] = useState(0);
  const room = useMemo(() => aJoinedCommonRoom(), []);
  const daemons: DaemonHost[] = [
    { instanceId: FIXTURE_DAEMON, label: `${FIXTURE_DAEMON} (this daemon)` },
  ];
  return (
    <AuthProvider>
      <SelectedDaemonProvider room={room} daemons={daemons}>
        <button
          type="button"
          data-testid={HARNESS_IDS.rebuildDaemons}
          onClick={() => setRebuilds((n) => n + 1)}
        >
          rebuild {rebuilds}
        </button>
        <ModelsAppPage onNavigate={() => {}} />
      </SelectedDaemonProvider>
    </AuthProvider>
  );
}

/** Lets the test navigate away from the screen while its reads are still in flight. */
function LeavableScreenHarness() {
  const [onScreen, setOnScreen] = useState(true);
  const room = useMemo(() => aJoinedCommonRoom(), []);
  const daemons: DaemonHost[] = [
    { instanceId: FIXTURE_DAEMON, label: `${FIXTURE_DAEMON} (this daemon)` },
  ];
  return (
    <AuthProvider>
      <SelectedDaemonProvider room={room} daemons={daemons}>
        <button
          type="button"
          data-testid={HARNESS_IDS.leaveScreen}
          onClick={() => setOnScreen(false)}
        >
          leave
        </button>
        {onScreen ? <ModelsAppPage onNavigate={() => {}} /> : null}
      </SelectedDaemonProvider>
    </AuthProvider>
  );
}

/** Holds the fan-out while the common room is down, and shows what a Refresh reported. */
function DisconnectedRefreshHarness() {
  const registry = useModelRegistryFanOut();
  return (
    <>
      <button
        type="button"
        data-testid={HARNESS_IDS.refresh}
        onClick={() => void registry.refreshProvider(OLLAMA)}
      >
        refresh
      </button>
      <span data-testid={HARNESS_IDS.refreshError}>
        {registry.providerErrors.get(providerRowKey(OLLAMA)) ?? ""}
      </span>
    </>
  );
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

describe("ModelsFanOutLifecycleAcceptance — reading the fleet without re-reading it", () => {
  it("re-reads nothing when the daemon list is rebuilt with the same daemons", () => {
    // Given — the fleet has been read once
    const backend = aModelRegistryBackend({
      providers: [anOllamaProvider()],
      models: [anLlmModel({ loadState: ModelLoadState.NOT_LOADED })],
    });
    mountWithRpc(<RebuiltDaemonListHarness />, backend);
    page.row(QWEN).should("exist");

    // When — the daemon list is rebuilt three times, then the operator loads the model. The load's
    // own re-read is the second `ListModels`, and it can only be issued after the rebuilds have
    // already had their chance to issue theirs
    byTestId(HARNESS_IDS.rebuildDaemons).click().click().click();
    page.loadModel(QWEN);
    page.rowLoadState(QWEN).should("equal", "loaded");

    // Then — the initial read and the load's re-read; nothing from the rebuilds
    cy.wrap(backend).should((b) => {
      expect(b.callsTo(ModelRegistryService.method.listModels)).to.have.length(2);
    });
  });

  it("cancels a read still in flight when the screen is left", () => {
    // Given — a daemon whose model list has not answered, so the screen is still reading
    const { backend, read } = aRegistryWithANeverAnsweringModelList();
    mountWithRpc(<LeavableScreenHarness />, backend);
    page.emptyStateStatus().should("equal", "loading");

    // When
    byTestId(HARNESS_IDS.leaveScreen).click();

    // Then — the daemon is told to stop, rather than the answer being awaited by nobody
    cy.wrap(read).should((r) => {
      expect(r.cancelled).to.equal(true);
    });
  });

  it("reports a refresh asked for while the common room is down instead of doing nothing", () => {
    // Given — the hook holds a provider, but there is no connection to its daemon
    const backend = aModelRegistryBackend({ providers: [anOllamaProvider()], models: [] });
    mountWithRpc(
      <AuthProvider>
        <SelectedDaemonProvider room={null} daemons={[]}>
          <DisconnectedRefreshHarness />
        </SelectedDaemonProvider>
      </AuthProvider>,
      backend,
    );

    // When
    byTestId(HARNESS_IDS.refresh).click();

    // Then — the operator is told why nothing was re-enumerated, and no call was invented
    byTestId(HARNESS_IDS.refreshError).should(
      "have.text",
      `no connection to daemon ${FIXTURE_DAEMON}`,
    );
    cy.wrap(backend).should((b) => {
      expect(b.callsTo(ModelRegistryService.method.refreshProviderModels)).to.have.length(0);
    });
  });
});
