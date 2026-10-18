/**
 * Acceptance tests: starting a Muse Code terminal session from the New-session form.
 *
 * PRD: docs/ft/daemon/1-WIP/PRD-2026-10-18-provider-route-muse-cli.md
 */
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CreateSessionPane } from "../../src/components/sessions/CreateSessionPane";
import { SessionService } from "../../src/gen/session_pb";
import { ProjectService } from "../../src/gen/project_pb";
import { CatalogService } from "../../src/gen/catalog_pb";
import { SessionFilesService } from "../../src/gen/session_files_pb";
import { WorktreeService } from "../../src/gen/worktree_pb";
import { TEST_IDS, byTestId } from "../support/testIds";

describe("CreateSessionPane — Muse Code terminal sessions", () => {
  it("starts a muse-cli session with a Muse model", () => {
    cy.viewport(1280, 800);
    const backend = anInMemoryRpcBackend()
      .onUnary(SessionService.method.listSessions, () => ({ sessions: [] }))
      .onUnary(CatalogService.method.listSubagents, () => ({ subagents: [] }))
      .onUnary(ProjectService.method.listProjects, () => ({
        projects: [{ projectId: "proj-1", name: "Test Project", mainRepoPath: "/repo" }],
      }))
      .onUnary(CatalogService.method.listAgents, () => ({ agents: [] }))
      .onUnary(CatalogService.method.listTools, () => ({ tools: [] }))
      .onUnary(CatalogService.method.listAgentModels, (req) =>
        req.agent === "muse-cli"
          ? {
              models: [
                { id: "muse-spark-1.3", label: "Muse Spark 1.3" },
                { id: "muse-wonder-1.0", label: "Muse Wonder 1.0" },
              ],
              defaultModel: "muse-spark-1.3",
            }
          : { models: [], defaultModel: "" },
      )
      .onUnary(SessionService.method.startSession, () => ({ sessionId: "muse-1" }));
    const transport = backend.transport();
    cy.mount(
      <CreateSessionPane
        client={createClient(SessionService, transport)}
        projectClient={createClient(ProjectService, transport)}
        catalogClient={createClient(CatalogService, transport)}
        sessionFilesClient={createClient(SessionFilesService, transport)}
        worktreeClient={createClient(WorktreeService, transport)}
        sessionToken="tok"
        onCancel={cy.stub()}
        onCreated={cy.stub()}
      />,
    );

    byTestId(TEST_IDS.createSessionTypeMuseCliBtn).should("contain.text", "Muse Code").click();
    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-1");
    byTestId(TEST_IDS.createSessionModelSelect).select("muse-wonder-1.0");
    byTestId(TEST_IDS.createSessionSubmitBtn).should("not.be.disabled").click();

    cy.wrap(null).should(() => {
      const calls = backend.callsTo(SessionService.method.startSession);
      expect(calls).to.have.length(1);
      expect(calls[0].sessionType).to.eq("muse-cli");
      expect(calls[0].model).to.eq("muse-wonder-1.0");
    });
  });
});
