/**
 * Unit tests for CreateSessionPane — mounted in isolation with mocked RPC client.
 *
 * Tests cover: field visibility per session type, create button enabled state,
 * branch intent sub-fields, startSession call parameters, cancel, loading state,
 * and error handling.
 */
import React from "react";
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import {
  ConnectionService,
  StartSessionRequestSchema,
  StartSessionResponseSchema,
} from "../../src/gen/connection_pb";
import { CreateSessionPane } from "../../src/components/sessions/CreateSessionPane";
import {
  interceptListProjectBranches,
  interceptStartSession,
} from "../support/rpc/connectionRpcs";
import {
  listAgentModels,
  listAgents,
  listProjects,
  listSessions,
  listTools,
} from "../support/rpc/responses";
import { toArrayBuffer, decodeProtoRequestBody } from "../support/rpc/protoRpc";
import { TEST_IDS, byTestId } from "../support/testIds";

/** Model catalog the daemon advertises for the baseline agent (mirrors the curated Claude set). */
const AGENT_MODELS = [
  { id: "claude-opus-4-8", label: "Claude Opus 4.8" },
  { id: "claude-sonnet-4-6", label: "Claude Sonnet 4.6" },
  { id: "claude-haiku-4-5-20251001", label: "Claude Haiku 4.5" },
];
const DEFAULT_MODEL = "claude-opus-4-8";

// ---------------------------------------------------------------------------
// Test client (uses cy.intercept network layer)
// ---------------------------------------------------------------------------

function createTestClient() {
  const transport = createConnectTransport({
    baseUrl: `${window.location.origin}/rpc`,
    useBinaryFormat: true,
  });
  return createClient(ConnectionService, transport);
}

// ---------------------------------------------------------------------------
// RPC intercept helpers (baseline — one project, one agent, one tool)
// ---------------------------------------------------------------------------

const TEST_PROJECT = { projectId: "proj-test", name: "Test Project", mainRepoPath: "/home/dev/test" };
const TEST_AGENT = { id: "claude", label: "Claude (opus)" };
const TEST_TOOL_PATH = "/usr/bin/tddy-coder";

function interceptBaseline() {
  const projectsBody = toArrayBuffer(listProjects([TEST_PROJECT]));
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListProjects", (req) => {
    req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body: projectsBody });
  }).as("listProjects");

  const agentsBody = toArrayBuffer(listAgents([TEST_AGENT]));
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListAgents", (req) => {
    req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body: agentsBody });
  }).as("listAgents");

  const toolsBody = toArrayBuffer(listTools([{ path: TEST_TOOL_PATH, label: "tddy-coder" }]));
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListTools", (req) => {
    req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body: toolsBody });
  }).as("listTools");

  const modelsBody = toArrayBuffer(listAgentModels(AGENT_MODELS, DEFAULT_MODEL));
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListAgentModels", (req) => {
    req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body: modelsBody });
  }).as("listAgentModels");
}

// ---------------------------------------------------------------------------
// Mount helper
// ---------------------------------------------------------------------------

function mountCreateSessionPane(overrides: {
  onCancel?: () => void;
  onCreated?: (id: string) => void;
} = {}) {
  const client = createTestClient();
  const onCancel = overrides.onCancel ?? cy.stub().as("onCancel");
  const onCreated = overrides.onCreated ?? cy.stub().as("onCreated");
  cy.mount(
    <CreateSessionPane
      client={client}
      sessionToken="fake-token"
      onCancel={onCancel}
      onCreated={onCreated}
    />,
  );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("CreateSessionPane — tool session fields (default)", () => {
  beforeEach(() => {
    interceptBaseline();
  });

  it("renders the create session pane with Tool type selected by default", () => {
    mountCreateSessionPane();
    cy.wait(["@listProjects", "@listAgents", "@listTools"]);

    byTestId(TEST_IDS.createSessionPane).should("be.visible");
    byTestId(TEST_IDS.createSessionTypeToolBtn).should("have.attr", "aria-pressed", "true");
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).should("have.attr", "aria-pressed", "false");
  });

  it("shows agent select and recipe select for tool session type", () => {
    mountCreateSessionPane();
    cy.wait(["@listProjects", "@listAgents", "@listTools"]);

    byTestId(TEST_IDS.createSessionAgentSelect).should("be.visible");
    byTestId(TEST_IDS.createSessionRecipeSelect).should("be.visible");
  });

  it("shows a model select for tool session type sourced from the daemon", () => {
    mountCreateSessionPane();
    cy.wait(["@listProjects", "@listAgents", "@listTools", "@listAgentModels"]);

    byTestId(TEST_IDS.createSessionModelSelect).should("be.visible");
    byTestId(TEST_IDS.createSessionModelSelect).should("have.value", DEFAULT_MODEL);
  });

  it("does not show permission mode or initial prompt for tool session type", () => {
    mountCreateSessionPane();
    cy.wait(["@listProjects", "@listAgents", "@listTools"]);

    byTestId(TEST_IDS.createSessionPermissionModeSelect).should("not.exist");
    byTestId(TEST_IDS.createSessionInitialPromptInput).should("not.exist");
  });
});

describe("CreateSessionPane — claude-cli session fields", () => {
  beforeEach(() => {
    interceptBaseline();
  });

  it("switches to claude-cli fields when Claude CLI type button is clicked", () => {
    mountCreateSessionPane();
    cy.wait(["@listProjects", "@listAgents", "@listTools"]);

    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();

    byTestId(TEST_IDS.createSessionModelSelect).should("be.visible");
    byTestId(TEST_IDS.createSessionPermissionModeSelect).should("be.visible");
    byTestId(TEST_IDS.createSessionInitialPromptInput).should("be.visible");
  });

  it("hides agent select and recipe select when Claude CLI type is selected", () => {
    mountCreateSessionPane();
    cy.wait(["@listProjects", "@listAgents", "@listTools"]);

    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();

    byTestId(TEST_IDS.createSessionAgentSelect).should("not.exist");
    byTestId(TEST_IDS.createSessionRecipeSelect).should("not.exist");
  });

  it("model dropdown for claude-cli lists the daemon-advertised models", () => {
    mountCreateSessionPane();
    cy.wait(["@listProjects", "@listAgents", "@listTools"]);

    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();
    cy.wait("@listAgentModels");

    byTestId(TEST_IDS.createSessionModelSelect).within(() => {
      AGENT_MODELS.forEach((m) => {
        cy.get("option").should("contain.text", m.label);
      });
    });
  });

  it("permission mode dropdown includes all valid modes", () => {
    mountCreateSessionPane();
    cy.wait(["@listProjects", "@listAgents", "@listTools"]);

    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();

    byTestId(TEST_IDS.createSessionPermissionModeSelect).within(() => {
      cy.get("option[value='auto']").should("exist");
      cy.get("option[value='default']").should("exist");
      cy.get("option[value='acceptEdits']").should("exist");
      cy.get("option[value='plan']").should("exist");
      cy.get("option[value='bypassPermissions']").should("exist");
    });
  });
});

describe("CreateSessionPane — create button enabled state", () => {
  it("Create button is disabled when no project is selected", () => {
    const noProjectsBody = toArrayBuffer(listProjects([]));
    cy.intercept("POST", "**/rpc/connection.ConnectionService/ListProjects", (req) => {
      req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body: noProjectsBody });
    }).as("listProjects");

    const agentsBody = toArrayBuffer(listAgents([TEST_AGENT]));
    cy.intercept("POST", "**/rpc/connection.ConnectionService/ListAgents", (req) => {
      req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body: agentsBody });
    }).as("listAgents");

    const toolsBody = toArrayBuffer(listTools([{ path: TEST_TOOL_PATH, label: "tddy-coder" }]));
    cy.intercept("POST", "**/rpc/connection.ConnectionService/ListTools", (req) => {
      req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body: toolsBody });
    }).as("listTools");

    mountCreateSessionPane();
    cy.wait(["@listProjects", "@listAgents", "@listTools"]);

    byTestId(TEST_IDS.createSessionSubmitBtn).should("be.disabled");
  });

  it("Create button is enabled when projectId and agent are set (tool session)", () => {
    interceptBaseline();
    mountCreateSessionPane();
    cy.wait(["@listProjects", "@listAgents", "@listTools"]);

    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-test");
    byTestId(TEST_IDS.createSessionAgentSelect).select("claude");

    byTestId(TEST_IDS.createSessionSubmitBtn).should("not.be.disabled");
  });

  it("Create button is enabled when projectId and model are set (claude-cli session)", () => {
    interceptBaseline();
    mountCreateSessionPane();
    cy.wait(["@listProjects", "@listAgents", "@listTools"]);

    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();
    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-test");
    // Model defaults to the daemon-advertised default once the probe resolves — button enables.

    byTestId(TEST_IDS.createSessionSubmitBtn).should("not.be.disabled");
  });
});

describe("CreateSessionPane — branch intent sub-fields", () => {
  beforeEach(() => {
    interceptBaseline();
  });

  it("shows new branch name input when branch intent is 'new_branch_from_base'", () => {
    mountCreateSessionPane();
    cy.wait(["@listProjects", "@listAgents", "@listTools"]);

    byTestId(TEST_IDS.createSessionBranchIntentSelect).select("new_branch_from_base");

    byTestId(TEST_IDS.createSessionNewBranchNameInput).should("be.visible");
    byTestId(TEST_IDS.createSessionBranchToWorkOnSelect).should("not.exist");
  });

  it("triggers ListProjectBranches when switching to 'work_on_selected_branch' with a project selected", () => {
    interceptListProjectBranches(["origin/main", "origin/dev"]);
    mountCreateSessionPane();
    cy.wait(["@listProjects", "@listAgents", "@listTools"]);

    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-test");
    byTestId(TEST_IDS.createSessionBranchIntentSelect).select("work_on_selected_branch");

    cy.wait("@listProjectBranches");

    byTestId(TEST_IDS.createSessionBranchToWorkOnSelect).should("be.visible");
    byTestId(TEST_IDS.createSessionBranchToWorkOnSelect).within(() => {
      cy.get("option").should("contain.text", "origin/main");
      cy.get("option").should("contain.text", "origin/dev");
    });
    byTestId(TEST_IDS.createSessionNewBranchNameInput).should("not.exist");
  });
});

describe("CreateSessionPane — submit behaviour", () => {
  it("calls startSession with correct tool session parameters", () => {
    interceptBaseline();
    interceptStartSession("new-session-tool-0001");
    interceptListProjectBranches();

    const capturedReqs: StartSessionRequest[] = [];
    cy.intercept("POST", "**/rpc/connection.ConnectionService/StartSession", (req) => {
      capturedReqs.push(fromBinary(StartSessionRequestSchema, decodeProtoRequestBody(req.body)));
      req.continue();
    });

    const onCreated = cy.stub().as("onCreated");
    mountCreateSessionPane({ onCreated });
    cy.wait(["@listProjects", "@listAgents", "@listTools"]);

    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-test");
    byTestId(TEST_IDS.createSessionAgentSelect).select("claude");
    byTestId(TEST_IDS.createSessionRecipeSelect).select("tdd");

    byTestId(TEST_IDS.createSessionSubmitBtn).click();
    cy.wait("@startSession");

    cy.then(() => {
      expect(capturedReqs).to.have.length.at.least(1);
      const req = capturedReqs[0]!;
      expect(req.projectId).to.equal("proj-test");
      expect(req.agent).to.equal("claude");
      expect(req.recipe).to.equal("tdd");
      expect(req.toolPath).to.equal(TEST_TOOL_PATH);
      expect(req.sessionType).to.equal("");
      expect(req.model).to.equal(DEFAULT_MODEL);
    });

    cy.get("@onCreated").should("have.been.calledWith", "new-session-tool-0001");
  });

  it("calls startSession with correct claude-cli parameters including permissionMode and initialPrompt", () => {
    interceptBaseline();
    interceptStartSession("new-session-cli-0002");
    interceptListProjectBranches();

    const capturedReqs: StartSessionRequest[] = [];
    cy.intercept("POST", "**/rpc/connection.ConnectionService/StartSession", (req) => {
      capturedReqs.push(fromBinary(StartSessionRequestSchema, decodeProtoRequestBody(req.body)));
      req.continue();
    });

    const onCreated = cy.stub().as("onCreated");
    mountCreateSessionPane({ onCreated });
    cy.wait(["@listProjects", "@listAgents", "@listTools"]);

    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();
    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-test");
    byTestId(TEST_IDS.createSessionModelSelect).select("claude-sonnet-4-6");
    byTestId(TEST_IDS.createSessionPermissionModeSelect).select("acceptEdits");
    byTestId(TEST_IDS.createSessionInitialPromptInput).type("Hello, Claude!");

    byTestId(TEST_IDS.createSessionSubmitBtn).click();
    cy.wait("@startSession");

    cy.then(() => {
      expect(capturedReqs).to.have.length.at.least(1);
      const req = capturedReqs[0]!;
      expect(req.projectId).to.equal("proj-test");
      expect(req.sessionType).to.equal("claude-cli");
      expect(req.model).to.equal("claude-sonnet-4-6");
      expect(req.permissionMode).to.equal("acceptEdits");
      expect(req.initialPrompt).to.equal("Hello, Claude!");
      expect(req.agent).to.equal("");
      expect(req.toolPath).to.equal("");
    });

    cy.get("@onCreated").should("have.been.calledWith", "new-session-cli-0002");
  });

  it("disables the Create button while startSession is in flight", () => {
    interceptBaseline();

    // Delay the response long enough for the assertion to run before it settles.
    // flushSync() in handleSubmit guarantees submitting=true is rendered synchronously
    // on click, so the button is disabled the entire time the request is pending.
    const responseBody = toArrayBuffer(
      toBinary(StartSessionResponseSchema, create(StartSessionResponseSchema, { sessionId: "in-flight-check" })),
    );
    cy.intercept("POST", "**/rpc/connection.ConnectionService/StartSession", (req) => {
      req.reply({ delay: 3000, statusCode: 200, headers: { "Content-Type": "application/proto" }, body: responseBody });
    }).as("startSessionSlow");

    mountCreateSessionPane();
    cy.wait(["@listProjects", "@listAgents", "@listTools"]);

    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-test");
    byTestId(TEST_IDS.createSessionAgentSelect).select("claude");

    byTestId(TEST_IDS.createSessionSubmitBtn).click();

    // Button should be disabled while request is in flight
    byTestId(TEST_IDS.createSessionSubmitBtn).should("be.disabled");
  });

  it("calls onCancel when the Cancel button is clicked", () => {
    interceptBaseline();
    const onCancel = cy.stub().as("onCancel");
    mountCreateSessionPane({ onCancel });
    cy.wait(["@listProjects", "@listAgents", "@listTools"]);

    byTestId(TEST_IDS.createSessionCancelBtn).click();

    cy.get("@onCancel").should("have.been.calledOnce");
  });

  it("shows an error message when startSession fails and keeps the form open", () => {
    interceptBaseline();
    cy.intercept("POST", "**/rpc/connection.ConnectionService/StartSession", (req) => {
      req.reply({ statusCode: 500, body: "daemon error" });
    }).as("startSessionFail");

    mountCreateSessionPane();
    cy.wait(["@listProjects", "@listAgents", "@listTools"]);

    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-test");
    byTestId(TEST_IDS.createSessionAgentSelect).select("claude");

    byTestId(TEST_IDS.createSessionSubmitBtn).click();
    cy.wait("@startSessionFail");

    byTestId(TEST_IDS.createSessionError).should("be.visible");
    // onCreated never called
    byTestId(TEST_IDS.createSessionPane).should("be.visible");
  });
});

// ---------------------------------------------------------------------------
// Recipe dropdown + parent-picker tests
// ---------------------------------------------------------------------------

function interceptBaselineWithSessions(
  orchestratorSessions: { sessionId: string; recipe?: string; orchestratorSessionId?: string }[] = [],
) {
  interceptBaseline();

  // ListSessions is called by the new-session screen to populate the parent picker.
  const sessionsBody = toArrayBuffer(listSessions(orchestratorSessions.map((s) => ({
    sessionId: s.sessionId,
    status: "active",
    isActive: true,
    projectId: "proj-1",
    recipe: s.recipe ?? "",
    orchestratorSessionId: s.orchestratorSessionId ?? "",
  }))));
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListSessions", (req) => {
    req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body: sessionsBody });
  }).as("listSessions");
}

describe("CreateSessionPane — recipe dropdown", () => {
  beforeEach(() => {
    interceptBaseline();
  });

  it("renders a recipe <select> (not free-text input) for tool session type", () => {
    mountCreateSessionPane();
    cy.wait(["@listProjects", "@listAgents", "@listTools"]);

    // The new select must exist; the old free-text input must not exist
    byTestId(TEST_IDS.createSessionRecipeSelect).should("be.visible");
    byTestId(TEST_IDS.createSessionRecipeInput).should("not.exist");
  });

  it("recipe select lists all 8 workflow recipes", () => {
    mountCreateSessionPane();
    cy.wait(["@listProjects", "@listAgents", "@listTools"]);

    const expectedRecipes = [
      "tdd",
      "tdd-small",
      "bugfix",
      "free-prompting",
      "grill-me",
      "review",
      "merge-pr",
      "pr-stack",
    ];

    byTestId(TEST_IDS.createSessionRecipeSelect).within(() => {
      expectedRecipes.forEach((recipe) => {
        cy.get(`option[value='${recipe}']`).should("exist");
      });
    });
  });

  it("recipe select defaults to 'tdd' on mount", () => {
    mountCreateSessionPane();
    cy.wait(["@listProjects", "@listAgents", "@listTools"]);

    byTestId(TEST_IDS.createSessionRecipeSelect).should("have.value", "tdd");
  });

  it("startSession sends the selected recipe from the dropdown", () => {
    interceptStartSession("recipe-dropdown-test-sess");
    interceptListProjectBranches();

    const capturedReqs: StartSessionRequest[] = [];
    cy.intercept("POST", "**/rpc/connection.ConnectionService/StartSession", (req) => {
      capturedReqs.push(fromBinary(StartSessionRequestSchema, decodeProtoRequestBody(req.body)));
      req.continue();
    });

    mountCreateSessionPane();
    cy.wait(["@listProjects", "@listAgents", "@listTools"]);

    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-test");
    byTestId(TEST_IDS.createSessionAgentSelect).select("claude");
    byTestId(TEST_IDS.createSessionRecipeSelect).select("tdd-small");

    byTestId(TEST_IDS.createSessionSubmitBtn).click();
    cy.wait("@startSession");

    cy.then(() => {
      expect(capturedReqs).to.have.length.at.least(1);
      expect(capturedReqs[0]!.recipe).to.equal("tdd-small");
    });
  });

  it("recipe select is hidden when Claude CLI session type is selected", () => {
    mountCreateSessionPane();
    cy.wait(["@listProjects", "@listAgents", "@listTools"]);

    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();

    byTestId(TEST_IDS.createSessionRecipeSelect).should("not.exist");
  });
});

describe("CreateSessionPane — stack parent picker", () => {
  it("shows the parent picker <select> for tool sessions when orchestrators are available", () => {
    interceptBaselineWithSessions([{ sessionId: "orch-sess-1", recipe: "orchestrate-pr-stack" }]);

    mountCreateSessionPane();
    cy.wait(["@listProjects", "@listAgents", "@listTools", "@listSessions"]);

    byTestId(TEST_IDS.createSessionStackParentSelect).should("be.visible");
  });

  it("parent picker is hidden when no orchestrator sessions exist", () => {
    interceptBaselineWithSessions([]); // empty list of orchestrators

    mountCreateSessionPane();
    cy.wait(["@listProjects", "@listAgents", "@listTools", "@listSessions"]);

    byTestId(TEST_IDS.createSessionStackParentSelect).should("not.exist");
  });

  it("shows the parent picker for claude-cli sessions when PR-stack orchestrators are available", () => {
    interceptBaselineWithSessions([{ sessionId: "orch-for-cli", recipe: "orchestrate-pr-stack" }]);

    mountCreateSessionPane();
    cy.wait(["@listProjects", "@listAgents", "@listTools", "@listSessions"]);

    // Switch to claude-cli session type
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();

    // The parent picker must be visible for claude-cli sessions too
    byTestId(TEST_IDS.createSessionStackParentSelect).should("be.visible");
  });

  it("hides the parent picker for both session types when only non-orchestrator sessions exist", () => {
    // Given — a session with a plain 'tdd' recipe (not a PR-stack orchestrator)
    interceptBaselineWithSessions([{ sessionId: "tdd-sess-1", recipe: "tdd" }]);

    mountCreateSessionPane();
    cy.wait(["@listProjects", "@listAgents", "@listTools", "@listSessions"]);

    // Tool: no picker (only PR-stack orchestrators should appear)
    byTestId(TEST_IDS.createSessionStackParentSelect).should("not.exist");

    // Claude CLI: same — no picker for non-orchestrator sessions
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();
    byTestId(TEST_IDS.createSessionStackParentSelect).should("not.exist");
  });

  it("startSession sends stackParent when a parent is selected (tool session)", () => {
    interceptBaselineWithSessions([{ sessionId: "orch-parent-123", recipe: "orchestrate-pr-stack" }]);
    interceptStartSession("child-with-parent-sess");
    interceptListProjectBranches();

    const capturedReqs: StartSessionRequest[] = [];
    cy.intercept("POST", "**/rpc/connection.ConnectionService/StartSession", (req) => {
      capturedReqs.push(fromBinary(StartSessionRequestSchema, decodeProtoRequestBody(req.body)));
      req.continue();
    });

    mountCreateSessionPane();
    cy.wait(["@listProjects", "@listAgents", "@listTools", "@listSessions"]);

    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-test");
    byTestId(TEST_IDS.createSessionAgentSelect).select("claude");
    byTestId(TEST_IDS.createSessionStackParentSelect).select("orch-parent-123");

    byTestId(TEST_IDS.createSessionSubmitBtn).click();
    cy.wait("@startSession");

    cy.then(() => {
      expect(capturedReqs).to.have.length.at.least(1);
      // stackParent maps to the proto `stack_parent = 15` field on StartSessionRequest
      expect((capturedReqs[0]! as Record<string, unknown>)["stackParent"]).to.equal("orch-parent-123");
    });
  });

  it("startSession for claude-cli sends stackParent when a PR-stack parent is selected", () => {
    interceptBaselineWithSessions([{ sessionId: "orch-cli-parent", recipe: "orchestrate-pr-stack" }]);
    interceptStartSession("claude-cli-child-sess");
    interceptListProjectBranches();

    const capturedReqs: StartSessionRequest[] = [];
    cy.intercept("POST", "**/rpc/connection.ConnectionService/StartSession", (req) => {
      capturedReqs.push(fromBinary(StartSessionRequestSchema, decodeProtoRequestBody(req.body)));
      req.continue();
    });

    mountCreateSessionPane();
    cy.wait(["@listProjects", "@listAgents", "@listTools", "@listSessions"]);

    // Switch to claude-cli and fill required fields
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();
    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-test");
    byTestId(TEST_IDS.createSessionStackParentSelect).select("orch-cli-parent");

    byTestId(TEST_IDS.createSessionSubmitBtn).click();
    cy.wait("@startSession");

    cy.then(() => {
      expect(capturedReqs).to.have.length.at.least(1);
      const req = capturedReqs[0]!;
      // Claude CLI session type must be set
      expect(req.sessionType).to.equal("claude-cli");
      // stackParent must be included in the claude-cli StartSession call
      expect((req as Record<string, unknown>)["stackParent"]).to.equal("orch-cli-parent");
    });
  });
});

// ---------------------------------------------------------------------------
// Type alias to avoid import gymnastics above
// ---------------------------------------------------------------------------
type StartSessionRequest = import("../../src/gen/connection_pb").StartSessionRequest;
