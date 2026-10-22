/**
 * Acceptance tests: a tool session's agent run on a provider account.
 *
 * The provider picker appears for tool sessions too, asking `ListProviderRoutes` for the selected
 * agent; Create sends the account with the agent.
 *
 * PRD: docs/ft/coder/1-WIP/PRD-2026-10-18-provider-route-tool-route.md
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
import { TEST_IDS, byTestId } from "../support/testIds";

function aBackend(): InMemoryRpcBackend {
  return anInMemoryRpcBackend()
    .onUnary(SessionService.method.listSessions, () => ({ sessions: [] }))
    .onUnary(CatalogService.method.listSubagents, () => ({ subagents: [] }))
    .onUnary(ProjectService.method.listProjects, () => ({
      projects: [{ projectId: "proj-1", name: "Test Project", mainRepoPath: "/repo" }],
    }))
    .onUnary(CatalogService.method.listAgents, () => ({
      agents: [
        { id: "codex", label: "Codex" },
        { id: "cursor", label: "Cursor" },
      ],
    }))
    .onUnary(CatalogService.method.listTools, () => ({
      tools: [{ path: "/usr/bin/tddy-coder", label: "tddy-coder" }],
    }))
    .onUnary(CatalogService.method.listAgentModels, () => ({
      models: [{ id: "gpt-5", label: "GPT-5" }],
      defaultModel: "gpt-5",
    }))
    .onUnary(AccountsService.method.listProviderRoutes, (req) =>
      req.harness === "cursor"
        ? { harnessRoutable: false, harnessReason: "Cursor's agent cannot use another provider", options: [] }
        : {
            harnessRoutable: true,
            harnessReason: "",
            options: [
              { accountId: "meta-hackathon", label: "Meta hackathon", preset: "meta", supported: true, reason: "" },
            ],
          },
    )
    .onUnary(AccountsService.method.listProviderAccountModels, () => ({
      modelIds: ["muse-spark-1.3", "muse-wonder-1.0"],
    }))
    .onUnary(SessionService.method.startSession, () => ({ sessionId: "tool-routed-1" }));
}

function mountWith(backend: InMemoryRpcBackend) {
  const transport = backend.transport();
  cy.mount(
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
  );
}

describe("CreateSessionPane — a tool session's agent on a provider account", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  it("asks which accounts the selected agent can run on", () => {
    const backend = aBackend();
    mountWith(backend);

    byTestId(TEST_IDS.createSessionProviderSelect).should("have.value", "builtin");
    cy.wrap(null).should(() => {
      const harnesses = backend.callsTo(AccountsService.method.listProviderRoutes).map((c) => c.harness);
      expect(harnesses).to.include("codex");
    });
  });

  it("sends the account with a codex tool session", () => {
    const backend = aBackend();
    mountWith(backend);
    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-1");

    byTestId(TEST_IDS.createSessionProviderSelect).select("meta-hackathon");
    byTestId(TEST_IDS.createSessionModelSelect).select("muse-wonder-1.0");
    byTestId(TEST_IDS.createSessionSubmitBtn).should("not.be.disabled").click();

    cy.wrap(null).should(() => {
      const calls = backend.callsTo(SessionService.method.startSession);
      expect(calls).to.have.length(1);
      expect(calls[0].agent).to.eq("codex");
      expect(calls[0].providerAccountId).to.eq("meta-hackathon");
      expect(calls[0].model).to.eq("muse-wonder-1.0");
    });
  });

  it("shows why the cursor agent cannot run on a provider account", () => {
    mountWith(aBackend());

    byTestId(TEST_IDS.createSessionAgentSelect).select("cursor");

    byTestId(TEST_IDS.createSessionProviderUnavailable).should(
      "contain.text",
      "Cursor's agent cannot use another provider",
    );
  });
});
