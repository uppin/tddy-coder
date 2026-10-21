/**
 * Acceptance tests: starting an OpenCode terminal session from the New-session form.
 *
 * OpenCode is a third CLI session type beside Claude CLI and Cursor CLI. Its model list is the
 * daemon's (`ListAgentModels` for the `opencode-cli` pseudo-agent — `opencode models`, one
 * `provider/model` per line), and Create sends `session_type = "opencode-cli"`.
 *
 * PRD: docs/ft/daemon/1-WIP/PRD-2026-10-18-provider-route-opencode-cli.md
 */
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CreateSessionPane } from "../../src/components/sessions/CreateSessionPane";
import { SessionService } from "../../src/gen/session_pb";
import { ProjectService } from "../../src/gen/project_pb";
import { CatalogService } from "../../src/gen/catalog_pb";
import { SessionFilesService } from "../../src/gen/session_files_pb";
import { WorktreeService } from "../../src/gen/worktree_pb";
import { TEST_IDS, byTestId } from "../support/testIds";

const OPENCODE_MODELS = [
  { id: "fireworks/kimi-k2", label: "fireworks/kimi-k2" },
  { id: "openai/gpt-5.6", label: "openai/gpt-5.6" },
];

function aBackend(): InMemoryRpcBackend {
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
    .onUnary(CatalogService.method.listAgentModels, (req) =>
      req.agent === "opencode-cli"
        ? { models: OPENCODE_MODELS, defaultModel: "fireworks/kimi-k2" }
        : { models: [{ id: "opus", label: "Claude Opus" }], defaultModel: "opus" },
    )
    .onUnary(SessionService.method.startSession, () => ({ sessionId: "opencode-sess-1" }));
}

function mountWith(backend: InMemoryRpcBackend) {
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
}

describe("CreateSessionPane — OpenCode terminal sessions", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  it("offers OpenCode beside Claude CLI and Cursor CLI", () => {
    mountWith(aBackend());

    byTestId(TEST_IDS.createSessionTypeOpenCodeCliBtn).should("contain.text", "OpenCode");
  });

  it("lists the models the daemon advertises for OpenCode", () => {
    const backend = aBackend();
    mountWith(backend);

    byTestId(TEST_IDS.createSessionTypeOpenCodeCliBtn).click();

    byTestId(TEST_IDS.createSessionModelSelect).should("have.value", "fireworks/kimi-k2");
    cy.wrap(null).should(() => {
      const agents = backend.callsTo(CatalogService.method.listAgentModels).map((c) => c.agent);
      expect(agents).to.include("opencode-cli");
    });
  });

  it("starts an opencode-cli session with the chosen model", () => {
    const backend = aBackend();
    mountWith(backend);
    byTestId(TEST_IDS.createSessionTypeOpenCodeCliBtn).click();
    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-1");

    byTestId(TEST_IDS.createSessionModelSelect).select("openai/gpt-5.6");
    byTestId(TEST_IDS.createSessionSubmitBtn).should("not.be.disabled").click();

    cy.wrap(null).should(() => {
      const calls = backend.callsTo(SessionService.method.startSession);
      expect(calls).to.have.length(1);
      expect(calls[0].sessionType).to.eq("opencode-cli");
      expect(calls[0].model).to.eq("openai/gpt-5.6");
    });
  });

  it("offers Create Remote Branch for a new OpenCode branch, unchecked", () => {
    mountWith(aBackend());
    byTestId(TEST_IDS.createSessionTypeOpenCodeCliBtn).click();
    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-1");

    byTestId(TEST_IDS.createSessionNewBranchNameInput).type("feature/opencode");

    byTestId(TEST_IDS.createSessionCreateRemoteBranchToggle).should("not.be.checked");
  });

  it("sends createRemoteBranch = true for an opencode-cli session when the toggle is checked", () => {
    const backend = aBackend();
    mountWith(backend);
    byTestId(TEST_IDS.createSessionTypeOpenCodeCliBtn).click();
    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-1");

    byTestId(TEST_IDS.createSessionNewBranchNameInput).type("feature/opencode");
    byTestId(TEST_IDS.createSessionCreateRemoteBranchToggle).check();
    byTestId(TEST_IDS.createSessionSubmitBtn).should("not.be.disabled").click();

    cy.wrap(null).should(() => {
      const calls = backend.callsTo(SessionService.method.startSession);
      expect(calls).to.have.length(1);
      expect(calls[0].sessionType).to.eq("opencode-cli");
      expect(calls[0].createRemoteBranch).to.eq(true);
    });
  });
});
