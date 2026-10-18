/**
 * Acceptance tests: making a provider account the default for a harness, and the New-session form
 * pre-selecting it.
 *
 * PRD: docs/ft/daemon/1-WIP/PRD-2026-10-18-provider-route-default-route.md
 */
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { AccountsAppPage } from "../../src/components/accounts/AccountsAppPage";
import { CreateSessionPane } from "../../src/components/sessions/CreateSessionPane";
import { AccountsService, SyncStatus, type AccountSummary } from "../../src/gen/accounts_pb";
import { SessionService } from "../../src/gen/session_pb";
import { ProjectService } from "../../src/gen/project_pb";
import { CatalogService } from "../../src/gen/catalog_pb";
import { SessionFilesService } from "../../src/gen/session_files_pb";
import { WorktreeService } from "../../src/gen/worktree_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { accountsScreenPage } from "../support/pages/accountsScreenPage";
import { TEST_IDS, byTestId } from "../support/testIds";

function meta(defaultFor: string[]): AccountSummary {
  return {
    provider: "meta",
    accountId: "meta-hackathon",
    label: "Meta hackathon",
    subject: "",
    updatedAt: 1_760_000_000n,
    hasSecret: true,
    syncStatus: SyncStatus.UNSPECIFIED,
    preset: "meta",
    wireProtocols: ["anthropic-messages", "openai-responses", "openai-chat"],
    defaultFor,
  } as AccountSummary;
}

describe("Default provider account per harness", () => {
  it("marks an account as the Claude CLI default from its row", () => {
    const sets: { accountId: string; harnesses: string[] }[] = [];
    const backend = anInMemoryRpcBackend().implement(AccountsService, {
      listAccounts: () => ({
        providers: [{ provider: "meta", accounts: [meta([])] }],
        vaultLocked: false,
        vaultUninitialized: false,
      }),
      setProviderAccountDefaults: (req) => {
        sets.push({ accountId: req.accountId, harnesses: [...req.harnesses] });
        return { changed: [meta(req.harnesses)] };
      },
    });
    mountWithRpc(withSelectedDaemon(<AccountsAppPage onNavigate={() => {}} />), backend);

    accountsScreenPage.defaultForSelect("meta", "meta-hackathon").select(["claude-cli"]);
    accountsScreenPage.defaultForSave("meta", "meta-hackathon").click();

    cy.wrap(sets).should("deep.equal", [{ accountId: "meta-hackathon", harnesses: ["claude-cli"] }]);
  });

  it("pre-selects the person's default when a claude-cli session is created", () => {
    const transport = anInMemoryRpcBackend()
      .onUnary(SessionService.method.listSessions, () => ({ sessions: [] }))
      .onUnary(CatalogService.method.listSubagents, () => ({ subagents: [] }))
      .onUnary(ProjectService.method.listProjects, () => ({ projects: [] }))
      .onUnary(CatalogService.method.listAgents, () => ({ agents: [] }))
      .onUnary(CatalogService.method.listTools, () => ({ tools: [] }))
      .onUnary(CatalogService.method.listAgentModels, () => ({ models: [], defaultModel: "" }))
      .onUnary(AccountsService.method.listAccounts, () => ({
        providers: [{ provider: "meta", accounts: [meta(["claude-cli"])] }],
        vaultLocked: false,
        vaultUninitialized: false,
      }))
      .onUnary(AccountsService.method.listProviderRoutes, () => ({
        harnessRoutable: true,
        harnessReason: "",
        options: [
          { accountId: "meta-hackathon", label: "Meta hackathon", preset: "meta", supported: true, reason: "" },
        ],
      }))
      .onUnary(AccountsService.method.listProviderAccountModels, () => ({ modelIds: ["muse-spark-1.3"] }))
      .transport();
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

    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();

    byTestId(TEST_IDS.createSessionProviderSelect).should("have.value", "meta-hackathon");
  });
});
