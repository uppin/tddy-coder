/**
 * Acceptance tests: starting a Warp terminal session from the New-session form. Warp takes no
 * model and runs on the person's own Warp sign-in, so the form offers neither field for it.
 *
 * Feature: docs/ft/daemon/warp-cli-session.md
 */
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CreateSessionPane } from "../../src/components/sessions/CreateSessionPane";
import { SessionService } from "../../src/gen/session_pb";
import { ProjectService } from "../../src/gen/project_pb";
import { CatalogService } from "../../src/gen/catalog_pb";
import { AccountsService } from "../../src/gen/accounts_pb";
import { SessionFilesService } from "../../src/gen/session_files_pb";
import { WorktreeService } from "../../src/gen/worktree_pb";
import { TEST_IDS, byTestId } from "../support/testIds";

/** A daemon with one project, no models for Warp, and a person holding one provider account. */
function aDaemonWithOneProject() {
  return anInMemoryRpcBackend()
    .onUnary(SessionService.method.listSessions, () => ({ sessions: [] }))
    .onUnary(CatalogService.method.listSubagents, () => ({ subagents: [] }))
    .onUnary(ProjectService.method.listProjects, () => ({
      projects: [{ projectId: "proj-1", name: "Test Project", mainRepoPath: "/repo" }],
    }))
    .onUnary(CatalogService.method.listAgents, () => ({ agents: [] }))
    .onUnary(CatalogService.method.listTools, () => ({ tools: [] }))
    .onUnary(CatalogService.method.listAgentModels, () => ({ models: [], defaultModel: "" }))
    .onUnary(AccountsService.method.listProviderRoutes, () => ({
      harnessRoutable: true,
      harnessReason: "",
      options: [
        { accountId: "meta-hackathon", label: "Meta hackathon", preset: "meta", supported: true, reason: "" },
      ],
    }))
    .onUnary(AccountsService.method.listProviderAccountModels, () => ({ modelIds: [] }))
    .onUnary(SessionService.method.startSession, () => ({ sessionId: "warp-1" }));
}

function mountTheNewSessionForm(backend: ReturnType<typeof aDaemonWithOneProject>) {
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

describe("CreateSessionPane — Warp terminal sessions", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  it("starts a warp-cli session with no model", () => {
    // Given
    const backend = aDaemonWithOneProject();
    mountTheNewSessionForm(backend);

    // When
    byTestId(TEST_IDS.createSessionTypeWarpCliBtn).should("contain.text", "Warp").click();
    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-1");
    byTestId(TEST_IDS.createSessionSubmitBtn).should("not.be.disabled").click();

    // Then
    cy.wrap(null).should(() => {
      const calls = backend.callsTo(SessionService.method.startSession);
      expect(calls).to.have.length(1);
      expect(calls[0].sessionType).to.eq("warp-cli");
      expect(calls[0].model).to.eq("");
      expect(calls[0].providerAccountId).to.eq("");
    });
  });

  it("offers no model or provider field for Warp", () => {
    // Given
    mountTheNewSessionForm(aDaemonWithOneProject());

    // When
    byTestId(TEST_IDS.createSessionTypeWarpCliBtn).click();

    // Then
    byTestId(TEST_IDS.createSessionTypeWarpCliBtn).should("have.attr", "aria-pressed", "true");
    byTestId(TEST_IDS.createSessionModelSelect).should("not.exist");
    byTestId(TEST_IDS.createSessionProviderField).should("not.exist");
  });
});
