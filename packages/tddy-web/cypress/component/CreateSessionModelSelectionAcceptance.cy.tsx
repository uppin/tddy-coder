/**
 * Model selection for tool (tddy-coder) sessions in the New-session form.
 *
 * The model list for the selected agent is fetched on demand via the `ListAgentModels` RPC
 * (enumerated from the underlying agent command, or a curated tddy-core list), and the chosen
 * model is sent in `StartSession`. The claude-cli model dropdown is fed by the same RPC instead of
 * a hardcoded web constant. See docs/ft/web/tool-session-model-selection.md.
 *
 * Uses the in-memory ConnectRPC backend (not `cy.intercept`) so tests assert on the typed request
 * the component actually sent and observe the models the daemon advertised.
 */
import React from "react";
import { createClient, ConnectError, Code } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CreateSessionPane } from "../../src/components/sessions/CreateSessionPane";
import { ConnectionService } from "../../src/gen/connection_pb";
import { TEST_IDS, byTestId } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures — the models each agent advertises
// ---------------------------------------------------------------------------

const CLAUDE_MODELS = [
  { id: "opus", label: "Claude Opus" },
  { id: "sonnet", label: "Claude Sonnet" },
  { id: "haiku", label: "Claude Haiku" },
];
const CURSOR_MODELS = [
  { id: "auto", label: "Auto" },
  { id: "gpt-5.2", label: "GPT-5.2" },
  { id: "composer-2.5", label: "Composer 2.5" },
];
// Deliberately distinct from the (to-be-removed) hardcoded `CLAUDE_CLI_MODELS` web constant so this
// test only passes once the dropdown is actually sourced from the daemon, not the constant.
const CLAUDE_CLI_MODELS = [
  { id: "claude-opus-4-8", label: "Claude Opus 4.8" },
  { id: "daemon-advertised-cli-model", label: "Daemon-Advertised CLI Model" },
];

/** Baseline backend: two agents (claude first, so it auto-selects), a project, and a model catalog
 *  keyed by the requested agent. `cursorFails` makes the cursor probe reject (auth failure). */
function aBackendWithModels({ cursorFails = false }: { cursorFails?: boolean } = {}) {
  return anInMemoryRpcBackend()
    .onUnary(ConnectionService.method.listSessions, () => ({ sessions: [] }))
    .onUnary(ConnectionService.method.listSubagents, () => ({ subagents: [] }))
    .onUnary(ConnectionService.method.listProjects, () => ({
      projects: [{ projectId: "proj-1", name: "Test Project", mainRepoPath: "/repo" }],
    }))
    .onUnary(ConnectionService.method.listAgents, () => ({
      agents: [
        { id: "claude", label: "Claude" },
        { id: "cursor", label: "Cursor" },
      ],
    }))
    .onUnary(ConnectionService.method.listTools, () => ({
      tools: [{ path: "/usr/bin/tddy-coder", label: "tddy-coder" }],
    }))
    .onUnary(ConnectionService.method.startSession, () => ({ sessionId: "model-sess-1" }))
    .onUnary(ConnectionService.method.listAgentModels, (req) => {
      if (req.agent === "cursor") {
        if (cursorFails) {
          throw new ConnectError("cursor: not logged in", Code.FailedPrecondition);
        }
        return { models: CURSOR_MODELS, defaultModel: "composer-2.5" };
      }
      if (req.agent === "claude-cli") {
        return { models: CLAUDE_CLI_MODELS, defaultModel: "daemon-advertised-cli-model" };
      }
      return { models: CLAUDE_MODELS, defaultModel: "opus" };
    });
}

function mountWith(backend: InMemoryRpcBackend) {
  const client = createClient(ConnectionService, backend.transport());
  cy.mount(
    <CreateSessionPane client={client} sessionToken="tok" onCancel={cy.stub()} onCreated={cy.stub()} />,
  );
}

describe("CreateSessionPane — tool-session model selection", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  it("shows a model dropdown for tool sessions populated from the selected agent's advertised models", () => {
    // Given — a backend where the auto-selected agent (claude) advertises opus/sonnet/haiku
    mountWith(aBackendWithModels());

    // Then — the tool session type shows the model select with those models
    byTestId(TEST_IDS.createSessionModelSelect).should("be.visible");
    byTestId(TEST_IDS.createSessionModelSelect).within(() => {
      cy.get("option[value='opus']").should("exist");
      cy.get("option[value='sonnet']").should("exist");
      cy.get("option[value='haiku']").should("exist");
    });
  });

  it("preselects the agent's default model", () => {
    // Given
    mountWith(aBackendWithModels());

    // Then — the select value equals the daemon-advertised default_model
    byTestId(TEST_IDS.createSessionModelSelect).should("have.value", "opus");
  });

  it("repopulates and resets the model when the agent changes", () => {
    // Given — the claude agent's models are shown
    mountWith(aBackendWithModels());
    byTestId(TEST_IDS.createSessionModelSelect).should("have.value", "opus");

    // When — switching to the cursor agent
    byTestId(TEST_IDS.createSessionAgentSelect).select("cursor");

    // Then — the model options are cursor's, reset to cursor's default
    byTestId(TEST_IDS.createSessionModelSelect).should("have.value", "composer-2.5");
    byTestId(TEST_IDS.createSessionModelSelect).within(() => {
      cy.get("option[value='gpt-5.2']").should("exist");
      cy.get("option[value='opus']").should("not.exist");
    });
  });

  it("sends the selected model in StartSession for a tool session", () => {
    // Given
    const backend = aBackendWithModels();
    mountWith(backend);
    byTestId(TEST_IDS.createSessionModelSelect).should("have.value", "opus");

    // When — pick a project, a non-default model, and submit
    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-1");
    byTestId(TEST_IDS.createSessionAgentSelect).select("claude");
    byTestId(TEST_IDS.createSessionModelSelect).select("sonnet");
    byTestId(TEST_IDS.createSessionSubmitBtn).should("not.be.disabled").click();

    // Then — the typed StartSession request carried the chosen model on the tool path
    cy.wrap(null).should(() => {
      const calls = backend.callsTo(ConnectionService.method.startSession);
      expect(calls).to.have.length(1);
      expect(calls[0].sessionType).to.eq("");
      expect(calls[0].agent).to.eq("claude");
      expect(calls[0].model).to.eq("sonnet");
    });
  });

  it("populates the claude-cli model dropdown from the daemon", () => {
    // Given
    mountWith(aBackendWithModels());

    // When — switching to the Claude CLI session type
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();

    // Then — the model select lists the daemon-advertised claude-cli catalog (not the old constant)
    byTestId(TEST_IDS.createSessionModelSelect).should("have.value", "daemon-advertised-cli-model");
    byTestId(TEST_IDS.createSessionModelSelect).within(() => {
      cy.get("option[value='daemon-advertised-cli-model']").should("exist");
    });
  });

  it("requests the models for the selected agent", () => {
    // Given
    const backend = aBackendWithModels();
    mountWith(backend);

    // When — switching to the cursor agent
    byTestId(TEST_IDS.createSessionAgentSelect).select("cursor");
    byTestId(TEST_IDS.createSessionModelSelect).should("have.value", "composer-2.5");

    // Then — ListAgentModels was asked for both the initial (claude) and the cursor agent
    cy.wrap(null).should(() => {
      const agents = backend
        .callsTo(ConnectionService.method.listAgentModels)
        .map((c) => c.agent);
      expect(agents).to.include("claude");
      expect(agents).to.include("cursor");
    });
  });

  it("shows an error and disables Create when the model probe fails", () => {
    // Given — cursor's probe rejects (e.g. not logged in)
    mountWith(aBackendWithModels({ cursorFails: true }));
    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-1");

    // When — selecting the cursor agent whose probe fails
    byTestId(TEST_IDS.createSessionAgentSelect).select("cursor");

    // Then — the failure is surfaced inline and Create is disabled (no fallback model)
    byTestId(TEST_IDS.createSessionModelError).should("be.visible");
    byTestId(TEST_IDS.createSessionSubmitBtn).should("be.disabled");
  });
});
