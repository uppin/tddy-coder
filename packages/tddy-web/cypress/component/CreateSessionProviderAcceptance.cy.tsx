/**
 * Acceptance tests: choosing where a claude-cli session's Claude Code runs — Anthropic (built in)
 * or one of the person's provider accounts — in the New-session form.
 *
 * The options and whether each account can drive the harness come from the daemon
 * (`ListProviderRoutes`); the form only renders them. Choosing an account switches the model list
 * to that account's models (`ListProviderAccountModels`), and Create sends the account id.
 *
 * PRD: docs/ft/daemon/1-WIP/PRD-2026-10-18-provider-route-claude-route.md
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

const CLAUDE_CLI_MODELS = [
  { id: "opus", label: "Claude Opus" },
  { id: "sonnet", label: "Claude Sonnet" },
];
const META_MODELS = ["muse-spark-1.3", "muse-wonder-1.0"];

function aBackendWithAMetaAccount(): InMemoryRpcBackend {
  return anInMemoryRpcBackend()
    .onUnary(SessionService.method.listSessions, () => ({ sessions: [] }))
    .onUnary(CatalogService.method.listSubagents, () => ({ subagents: [] }))
    .onUnary(ProjectService.method.listProjects, () => ({
      projects: [{ projectId: "proj-1", name: "Test Project", mainRepoPath: "/repo" }],
    }))
    .onUnary(CatalogService.method.listAgents, () => ({ agents: [{ id: "claude", label: "Claude" }] }))
    .onUnary(CatalogService.method.listTools, () => ({
      tools: [{ path: "/usr/bin/tddy-coder", label: "tddy-coder" }],
    }))
    .onUnary(CatalogService.method.listAgentModels, () => ({
      models: CLAUDE_CLI_MODELS,
      defaultModel: "opus",
    }))
    .onUnary(AccountsService.method.listProviderRoutes, (req) =>
      req.harness.startsWith("cursor")
        ? {
            harnessRoutable: false,
            harnessReason: "Cursor's agent cannot use another provider",
            options: [],
          }
        : {
            harnessRoutable: true,
            harnessReason: "",
            options: [
              {
                accountId: "baseten-kimi",
                label: "Kimi on Baseten",
                preset: "baseten",
                supported: false,
                reason: "Claude Code needs an endpoint that serves Anthropic Messages",
              },
              {
                accountId: "meta-hackathon",
                label: "Meta hackathon",
                preset: "meta",
                supported: true,
                reason: "",
              },
            ],
          },
    )
    .onUnary(AccountsService.method.listProviderAccountModels, () => ({ modelIds: META_MODELS }))
    .onUnary(SessionService.method.startSession, () => ({ sessionId: "routed-sess-1" }));
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

describe("CreateSessionPane — the provider a claude-cli session runs on", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  it("defaults to the built-in vendor and offers the accounts the daemon marks usable", () => {
    mountWith(aBackendWithAMetaAccount());
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();

    byTestId(TEST_IDS.createSessionProviderSelect).should("have.value", "builtin");
    byTestId(TEST_IDS.createSessionProviderSelect).within(() => {
      cy.get("option[value='meta-hackathon']").should("not.be.disabled");
      cy.get("option[value='baseten-kimi']").should("be.disabled");
    });
  });

  it("lists the chosen account's models once an account is chosen", () => {
    mountWith(aBackendWithAMetaAccount());
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();

    byTestId(TEST_IDS.createSessionProviderSelect).select("meta-hackathon");

    byTestId(TEST_IDS.createSessionModelSelect).within(() => {
      cy.get("option[value='muse-wonder-1.0']").should("exist");
      cy.get("option[value='opus']").should("not.exist");
    });
  });

  it("sends the account and its model when the session is created", () => {
    const backend = aBackendWithAMetaAccount();
    mountWith(backend);
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();
    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-1");

    byTestId(TEST_IDS.createSessionProviderSelect).select("meta-hackathon");
    byTestId(TEST_IDS.createSessionModelSelect).select("muse-wonder-1.0");
    byTestId(TEST_IDS.createSessionSubmitBtn).should("not.be.disabled").click();

    cy.wrap(null).should(() => {
      const calls = backend.callsTo(SessionService.method.startSession);
      expect(calls).to.have.length(1);
      expect(calls[0].sessionType).to.eq("claude-cli");
      expect(calls[0].providerAccountId).to.eq("meta-hackathon");
      expect(calls[0].providerBuiltIn).to.eq(false);
      expect(calls[0].model).to.eq("muse-wonder-1.0");
    });
  });

  it("shows why Cursor cannot run on a provider account instead of a picker", () => {
    mountWith(aBackendWithAMetaAccount());

    byTestId(TEST_IDS.createSessionTypeCursorCliBtn).click();

    byTestId(TEST_IDS.createSessionProviderSelect).should("not.exist");
    byTestId(TEST_IDS.createSessionProviderUnavailable).should(
      "contain.text",
      "Cursor's agent cannot use another provider",
    );
  });
});
