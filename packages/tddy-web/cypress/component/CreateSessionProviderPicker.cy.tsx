/**
 * Acceptance: the create-session pane asks for provider routes only for an agent whose catalog
 * row says a route can point it elsewhere (`#agent-tools 12/12`).
 *
 * `ListAgents` rows carry each agent's kind, harness and `routeProviders`, read from its spec. A
 * specialized agent definition (a MODEL row) and an agent that always runs on its own vendor have
 * none, so the pane neither offers the provider field nor asks `ListProviderRoutes` for them.
 *
 * PRD: docs/ft/daemon/1-WIP/PRD-2026-10-31-coding-agent-registry.md
 * Closes docs/dev/todo/2026-10-22-tool-session-provider-picker-needs-harness-metadata.md
 */
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CreateSessionPane } from "../../src/components/sessions/CreateSessionPane";
import { SessionService } from "../../src/gen/session_pb";
import { ProjectService } from "../../src/gen/project_pb";
import { CatalogService } from "../../src/gen/catalog_pb";
import { AccountsService } from "../../src/gen/accounts_pb";
import { SessionFilesService } from "../../src/gen/session_files_pb";
import { WorktreeService } from "../../src/gen/worktree_pb";
import { AgentKind } from "../../src/gen/types_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { TEST_IDS, byTestId } from "../support/testIds";

/** A coding agent from the registry, routable to `routeProviders`. */
function aCodingAgent(id: string, routeProviders: string[]) {
  return { id, label: id, kind: AgentKind.CODING_AGENT, harness: id, routeProviders };
}

/** A specialized agent definition: no harness, nothing to route. */
function aDefinition(id: string) {
  return { id, label: id, kind: AgentKind.MODEL, harness: "", routeProviders: [] };
}

/**
 * A daemon listing Claude (routable to Anthropic) first and the `reviewer` definition second, and
 * answering every `ListProviderRoutes` with one usable account — so any request the pane makes is
 * visible as a call and as a shown field.
 */
function aBackend(): InMemoryRpcBackend {
  return anInMemoryRpcBackend()
    .onUnary(SessionService.method.listSessions, () => ({ sessions: [] }))
    .onUnary(CatalogService.method.listSubagents, () => ({ subagents: [] }))
    .onUnary(ProjectService.method.listProjects, () => ({
      projects: [{ projectId: "proj-1", name: "Test Project", mainRepoPath: "/repo" }],
    }))
    .onUnary(CatalogService.method.listAgents, () => ({
      agents: [aCodingAgent("claude", ["anthropic"]), aDefinition("reviewer")],
    }))
    .onUnary(CatalogService.method.listTools, () => ({
      tools: [{ path: "/usr/bin/tddy-coder", label: "tddy-coder" }],
    }))
    .onUnary(CatalogService.method.listAgentModels, () => ({
      models: [{ id: "opus", label: "Opus" }],
      defaultModel: "opus",
    }))
    .onUnary(AccountsService.method.listProviderRoutes, () => ({
      harnessRoutable: true,
      harnessReason: "",
      options: [
        { accountId: "anthropic-team", label: "Anthropic team", preset: "anthropic", supported: true, reason: "" },
      ],
    }))
    .onUnary(AccountsService.method.listProviderAccountModels, () => ({ modelIds: [] }));
}

function mountWith(backend: InMemoryRpcBackend) {
  const transport = backend.transport();
  mountWithRpc(
    <CreateSessionPane
      client={createClient(SessionService, transport)}
      projectClient={createClient(ProjectService, transport)}
      catalogClient={createClient(CatalogService, transport)}
      accountsClient={createClient(AccountsService, transport)}
      sessionFilesClient={createClient(SessionFilesService, transport)}
      worktreeClient={createClient(WorktreeService, transport)}
      sessionToken="tok"
      onCancel={cy.stub()}
      onCreated={cy.stub()}
    />,
    backend,
  );
}

function routeRequestsTo(backend: InMemoryRpcBackend): string[] {
  return backend.callsTo(AccountsService.method.listProviderRoutes).map((call) => call.harness);
}

describe("CreateSessionPane — provider routes follow the catalog's route support", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  it("does not request provider routes for an agent whose route support is empty", () => {
    // Given — Claude, routable, is selected first: the positive control
    const backend = aBackend();
    mountWith(backend);
    byTestId(TEST_IDS.createSessionProviderField).should("exist");

    // When
    byTestId(TEST_IDS.createSessionAgentSelect).select("reviewer");

    // Then — no field for the definition, and the only harness ever asked about is Claude's
    byTestId(TEST_IDS.createSessionProviderField).should("not.exist");
    cy.wrap(null).should(() => {
      expect(new Set(routeRequestsTo(backend))).to.deep.equal(new Set(["claude"]));
    });
  });
});
