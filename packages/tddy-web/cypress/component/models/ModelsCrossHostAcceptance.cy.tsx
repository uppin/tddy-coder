/**
 * Acceptance: the Models & Agents screen merges the registries of **every** common-room daemon into
 * one table, routes a per-model action to that model's **owning** daemon rather than the selected
 * one, and degrades a single unreachable daemon into one error row instead of an empty page.
 *
 * Each daemon answers with its own in-memory backend (`mountWithPerDaemonLiveKitRpc`), so an RPC
 * landing on host B's backend is proof of routing to host B.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md (AC2, AC5, AC12).
 */

import React from "react";
import { Code, ConnectError } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { ModelLoadState, ModelRegistryService } from "../../../src/gen/models_pb";
import { ModelsAppPage } from "../../../src/components/models/ModelsAppPage";
import { daemonRpcIdentity, type DaemonHost } from "../../../src/lib/participantRole";
import { withSelectedDaemon } from "../../support/rpc/withSelectedDaemon";
import { mountWithPerDaemonLiveKitRpc } from "../../support/rpc/perDaemonLiveKitRpc";
import {
  aModelRegistryBackend,
  anAssistant,
  anLlmModel,
  anOllamaProvider,
} from "../../support/rpc/modelRegistryBackend";
import {
  modelsScreenPage as page,
  type AssistantRef,
  type ModelRef,
  type ProviderRef,
} from "../../support/pages/modelsScreenPage";
import { recordedFields } from "../../support/rpc/recordedRequests";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/** Host A — selected first by `SelectedDaemonProvider`. */
const HOST_A: DaemonHost = { instanceId: "workstation-1", label: "workstation-1 (this daemon)" };
/** Host B — a peer daemon, never selected in these specs. */
const HOST_B: DaemonHost = { instanceId: "server-2", label: "server-2" };

const QWEN_ON_A: ModelRef = {
  daemonInstanceId: HOST_A.instanceId,
  providerId: "prov-ollama",
  modelId: "qwen3:32b",
};
const LLAMA_ON_B: ModelRef = {
  daemonInstanceId: HOST_B.instanceId,
  providerId: "prov-ollama-b",
  modelId: "llama3.3:70b",
};

/** Host A's registry: one Ollama provider serving one not-resident model. */
function backendForHostA() {
  return aModelRegistryBackend({
    providers: [anOllamaProvider({ daemonInstanceId: HOST_A.instanceId })],
    models: [anLlmModel({ daemonInstanceId: HOST_A.instanceId })],
  });
}

/** Host B's registry: a different provider serving a different model. */
function backendForHostB() {
  return aModelRegistryBackend({
    providers: [
      anOllamaProvider({
        providerId: LLAMA_ON_B.providerId,
        label: "server-2 Ollama",
        daemonInstanceId: HOST_B.instanceId,
      }),
    ],
    models: [
      anLlmModel({
        modelId: LLAMA_ON_B.modelId,
        providerId: LLAMA_ON_B.providerId,
        label: "Llama 3.3 70B",
        daemonInstanceId: HOST_B.instanceId,
        loadState: ModelLoadState.NOT_LOADED,
      }),
    ],
  });
}

/**
 * The same provider id on both hosts — which is the normal case, since ids are minted per daemon:
 * every host that runs Ollama has a `prov-ollama`.
 */
const SHARED_PROVIDER_ID = "prov-ollama";
const OLLAMA_ON_A: ProviderRef = {
  daemonInstanceId: HOST_A.instanceId,
  providerId: SHARED_PROVIDER_ID,
};
const OLLAMA_ON_B: ProviderRef = {
  daemonInstanceId: HOST_B.instanceId,
  providerId: SHARED_PROVIDER_ID,
};
const QWEN_ON_B: ModelRef = {
  daemonInstanceId: HOST_B.instanceId,
  providerId: SHARED_PROVIDER_ID,
  modelId: "qwen3:32b",
};

/** Host B's registry, using the *same* provider id as host A's, with its enumeration failing. */
function backendForHostBWithFailingProvider() {
  return aModelRegistryBackend({
    providers: [
      anOllamaProvider({
        providerId: SHARED_PROVIDER_ID,
        label: "server-2 Ollama",
        daemonInstanceId: HOST_B.instanceId,
        enumerationError: "connection refused: http://localhost:11434/api/tags",
      }),
    ],
    models: [anLlmModel({ daemonInstanceId: HOST_B.instanceId })],
  });
}

/**
 * The same assistant name on both hosts — the normal case, since a name is unique *per daemon*:
 * every host that reviews code may define a `reviewer`.
 */
const SHARED_ASSISTANT_NAME = "reviewer";
const REVIEWER_ON_A: AssistantRef = {
  daemonInstanceId: HOST_A.instanceId,
  name: SHARED_ASSISTANT_NAME,
};
const REVIEWER_ON_B: AssistantRef = {
  daemonInstanceId: HOST_B.instanceId,
  name: SHARED_ASSISTANT_NAME,
};

/** Host A's registry, with a `reviewer` assistant assigned read-only tools. */
function backendForHostAWithReviewer() {
  return aModelRegistryBackend({
    providers: [anOllamaProvider({ daemonInstanceId: HOST_A.instanceId })],
    models: [anLlmModel({ daemonInstanceId: HOST_A.instanceId })],
    assistants: [
      anAssistant({
        name: SHARED_ASSISTANT_NAME,
        label: "workstation-1 Reviewer",
        tools: ["Read", "Grep"],
        daemonInstanceId: HOST_A.instanceId,
      }),
    ],
  });
}

/** Host B's registry, with its *own* `reviewer` — same name, different tools. */
function backendForHostBWithReviewer() {
  return aModelRegistryBackend({
    providers: [
      anOllamaProvider({
        providerId: LLAMA_ON_B.providerId,
        label: "server-2 Ollama",
        daemonInstanceId: HOST_B.instanceId,
      }),
    ],
    models: [],
    assistants: [
      anAssistant({
        assistantId: "asst-b1",
        name: SHARED_ASSISTANT_NAME,
        label: "server-2 Reviewer",
        providerId: LLAMA_ON_B.providerId,
        modelId: LLAMA_ON_B.modelId,
        tools: ["Read", "Shell"],
        daemonInstanceId: HOST_B.instanceId,
      }),
    ],
  });
}

/** A daemon whose registry cannot be read at all. */
function anUnreachableBackend() {
  return anInMemoryRpcBackend()
    .onUnary(ModelRegistryService.method.listProviders, () => {
      throw new ConnectError("common room peer is unreachable", Code.Unavailable);
    })
    .onUnary(ModelRegistryService.method.listModels, () => {
      throw new ConnectError("common room peer is unreachable", Code.Unavailable);
    })
    .onUnary(ModelRegistryService.method.listAssistants, () => {
      throw new ConnectError("common room peer is unreachable", Code.Unavailable);
    })
    .onUnary(ModelRegistryService.method.listAssignableTools, () => {
      throw new ConnectError("common room peer is unreachable", Code.Unavailable);
    });
}

function mountCrossHost(
  backendA: ReturnType<typeof aModelRegistryBackend>,
  backendB: ReturnType<typeof aModelRegistryBackend>,
) {
  return mountWithPerDaemonLiveKitRpc(
    withSelectedDaemon(<ModelsAppPage onNavigate={cy.stub()} />, [HOST_A, HOST_B]),
    {
      [daemonRpcIdentity(HOST_A.instanceId)]: backendA,
      [daemonRpcIdentity(HOST_B.instanceId)]: backendB,
    },
    { httpBackend: backendA },
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

describe("ModelsCrossHostAcceptance — one table across every connected daemon", () => {
  it("lists models from both connected daemons, each row naming its owning daemon", () => {
    // Given — two daemons, each with its own registry
    // When
    mountCrossHost(backendForHostA(), backendForHostB());

    // Then
    page.rowDaemon(QWEN_ON_A).should("equal", HOST_A.instanceId);
    page.rowDaemon(LLAMA_ON_B).should("equal", HOST_B.instanceId);
  });

  it("sends a load for a model owned by an unselected daemon to that daemon", () => {
    // Given — host A is selected; the model to load lives on host B
    const backendA = backendForHostA();
    const backendB = backendForHostB();
    mountCrossHost(backendA, backendB);

    // When
    page.loadModel(LLAMA_ON_B);

    // Then — host B received the load; host A received none
    cy.wrap(backendB).should((b) => {
      expect(recordedFields(b.callsTo(ModelRegistryService.method.loadModel))).to.deep.equal([
        {
          sessionToken: "fake-token",
          providerId: LLAMA_ON_B.providerId,
          modelId: LLAMA_ON_B.modelId,
        },
      ]);
    });
    cy.wrap(backendA).should((b) => {
      expect(b.callsTo(ModelRegistryService.method.loadModel)).to.have.length(0);
    });
  });

  it("renders a provider's enumeration error against the daemon that reported it", () => {
    // Given — both daemons run a provider called `prov-ollama`; only host B's is failing
    mountCrossHost(backendForHostA(), backendForHostBWithFailingProvider());

    // When / Then — the failure belongs to host B's row. The waiting assertion proves host B's read
    // is in, so the `timeout: 0` absence cannot pass merely because nothing has arrived yet
    page
      .providerError(OLLAMA_ON_B)
      .should("contain.text", "connection refused: http://localhost:11434/api/tags");
    page.providerError(OLLAMA_ON_A, { timeout: 0 }).should("not.exist");
  });

  it("marks only the failing daemon's models stale when both daemons share a provider id", () => {
    // Given — host B's `prov-ollama` failed to enumerate; host A's `prov-ollama` is healthy
    mountCrossHost(backendForHostA(), backendForHostBWithFailingProvider());

    // When / Then
    page.rowIsStale(QWEN_ON_B).should("equal", "true");
    page.rowIsStale(QWEN_ON_A).should("equal", "false");
  });

  it("sends a provider deletion to the daemon that owns the provider", () => {
    // Given — host A is selected; the provider to remove lives on host B
    const backendA = backendForHostA();
    const backendB = backendForHostB();
    mountCrossHost(backendA, backendB);

    // When
    page.deleteProvider({
      daemonInstanceId: HOST_B.instanceId,
      providerId: LLAMA_ON_B.providerId,
    });

    // Then — host B removed its own provider; host A, whose registry is untouched, was asked
    // nothing. A write routed to the selected daemon would delete the wrong host's provider
    cy.wrap(backendB).should((b) => {
      expect(recordedFields(b.callsTo(ModelRegistryService.method.deleteProvider))).to.deep.equal([
        { sessionToken: "fake-token", providerId: LLAMA_ON_B.providerId },
      ]);
    });
    cy.wrap(backendA).should((b) => {
      expect(b.callsTo(ModelRegistryService.method.deleteProvider)).to.have.length(0);
    });
  });

  it("keeps two daemons' identically named assistants apart, each with its own tools", () => {
    // Given — both daemons define a `reviewer`; assistant names are unique per daemon, not fleetwide
    // When
    mountCrossHost(backendForHostAWithReviewer(), backendForHostBWithReviewer());

    // Then — two distinct rows, each carrying the tools its own daemon assigned. One id for both
    // would render whichever daemon answered first against the other daemon's name
    page.assistantTools(REVIEWER_ON_A).should("deep.equal", ["Read", "Grep"]);
    page.assistantTools(REVIEWER_ON_B).should("deep.equal", ["Read", "Shell"]);
    page.assistantRow(REVIEWER_ON_B).should("contain.text", "server-2 Reviewer");
  });

  it("sends an assistant deletion to the daemon that owns the assistant", () => {
    // Given — both daemons define a `reviewer`, and host A is the selected one
    const backendA = backendForHostAWithReviewer();
    const backendB = backendForHostBWithReviewer();
    mountCrossHost(backendA, backendB);

    // When — the operator deletes host B's
    page.deleteAssistant(REVIEWER_ON_B);

    // Then — host B deleted its own assistant id; host A's `reviewer` was never addressed
    cy.wrap(backendB).should((b) => {
      expect(recordedFields(b.callsTo(ModelRegistryService.method.deleteAssistant))).to.deep.equal([
        { sessionToken: "fake-token", assistantId: "asst-b1" },
      ]);
    });
    cy.wrap(backendA).should((b) => {
      expect(b.callsTo(ModelRegistryService.method.deleteAssistant)).to.have.length(0);
    });
    page.assistantRow(REVIEWER_ON_A).should("exist");
  });

  it("renders an error row for an unreachable daemon while the other daemon's models still list", () => {
    // Given — host B cannot be read at all
    mountCrossHost(backendForHostA(), anUnreachableBackend());

    // Then — host A's model is present and host B is called out as failed. The waiting
    // `daemonError` assertion proves host B's read has already come back, so the `timeout: 0`
    // absence below cannot pass merely because that daemon had not answered yet
    page.row(QWEN_ON_A).should("exist");
    page.daemonError(HOST_B.instanceId).should("contain.text", "unreachable");
    page.row(LLAMA_ON_B, { timeout: 0 }).should("not.exist");
  });
});
