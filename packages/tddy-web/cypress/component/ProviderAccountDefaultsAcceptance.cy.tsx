/**
 * Acceptance tests: making a provider account the default for a harness, and the New-session form
 * pre-selecting it.
 *
 * Feature: docs/ft/daemon/provider-accounts.md (A person's default per harness)
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

function aProviderAccount(preset: string, accountId: string, defaultFor: string[]): AccountSummary {
  return {
    provider: preset,
    accountId,
    label: accountId,
    subject: "",
    updatedAt: 1_760_000_000n,
    hasSecret: true,
    syncStatus: SyncStatus.UNSPECIFIED,
    preset,
    wireProtocols: ["anthropic-messages", "openai-responses", "openai-chat"],
    defaultFor,
  } as AccountSummary;
}

function meta(defaultFor: string[]): AccountSummary {
  return { ...aProviderAccount("meta", "meta-hackathon", defaultFor), label: "Meta hackathon" };
}

/** Mounts the New-session form for a person whose Meta account is the default for `defaultFor`. */
function mountCreateSessionPaneWithTheDefault(defaultFor: string[]) {
  const transport = anInMemoryRpcBackend()
    .onUnary(SessionService.method.listSessions, () => ({ sessions: [] }))
    .onUnary(CatalogService.method.listSubagents, () => ({ subagents: [] }))
    .onUnary(ProjectService.method.listProjects, () => ({ projects: [] }))
    .onUnary(CatalogService.method.listAgents, () => ({ agents: [] }))
    .onUnary(CatalogService.method.listTools, () => ({ tools: [] }))
    .onUnary(CatalogService.method.listAgentModels, () => ({ models: [], defaultModel: "" }))
    .onUnary(AccountsService.method.listAccounts, () => ({
      providers: [{ provider: "meta", accounts: [meta(defaultFor)] }],
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

  it("marks a Meta account as the Muse Code default from its row", () => {
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

    accountsScreenPage.defaultForSelect("meta", "meta-hackathon").select(["Muse Code"]);
    accountsScreenPage.defaultForSave("meta", "meta-hackathon").click();

    cy.wrap(sets).should("deep.equal", [{ accountId: "meta-hackathon", harnesses: ["muse-cli"] }]);
  });

  it("clears a default from the row that held it when another account takes it", () => {
    const backend = anInMemoryRpcBackend().implement(AccountsService, {
      listAccounts: () => ({
        providers: [
          { provider: "meta", accounts: [meta(["claude-cli"])] },
          { provider: "fireworks", accounts: [aProviderAccount("fireworks", "fw", [])] },
        ],
        vaultLocked: false,
        vaultUninitialized: false,
      }),
      setProviderAccountDefaults: (req) => ({
        changed: [aProviderAccount("fireworks", "fw", [...req.harnesses]), meta([])],
      }),
    });
    mountWithRpc(withSelectedDaemon(<AccountsAppPage onNavigate={() => {}} />), backend);
    accountsScreenPage.defaultForSelect("meta", "meta-hackathon").invoke("val").should("deep.equal", ["claude-cli"]);

    accountsScreenPage.defaultForSelect("fireworks", "fw").select(["claude-cli"]);
    accountsScreenPage.defaultForSave("fireworks", "fw").click();

    accountsScreenPage.defaultForSelect("meta", "meta-hackathon").invoke("val").should("deep.equal", []);
  });

  it("pre-selects the person's default when a claude-cli session is created", () => {
    mountCreateSessionPaneWithTheDefault(["claude-cli"]);

    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();

    byTestId(TEST_IDS.createSessionProviderSelect).should("have.value", "meta-hackathon");
  });

  it("pre-selects the person's default when a muse-cli session is created", () => {
    mountCreateSessionPaneWithTheDefault(["muse-cli"]);

    byTestId(TEST_IDS.createSessionTypeMuseCliBtn).click();

    byTestId(TEST_IDS.createSessionProviderSelect).should("have.value", "meta-hackathon");
  });
});
