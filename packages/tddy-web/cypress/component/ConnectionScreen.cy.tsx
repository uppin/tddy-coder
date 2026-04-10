import React from "react";
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import { ConnectionScreen } from "../../src/components/ConnectionScreen";
import {
  ListToolsResponseSchema,
  ToolInfoSchema,
  ListAgentsResponseSchema,
  AgentInfoSchema,
  ListSessionsResponseSchema,
  SessionEntrySchema,
  ListProjectsResponseSchema,
  ProjectEntrySchema,
  ListEligibleDaemonsResponseSchema,
  EligibleDaemonEntrySchema,
  StartSessionRequestSchema,
  StartSessionResponseSchema,
  ConnectSessionRequestSchema,
  ConnectSessionResponseSchema,
  ResumeSessionResponseSchema,
  DeleteSessionRequestSchema,
} from "../../src/gen/connection_pb";
import {
  GetAuthStatusResponseSchema,
  GitHubUserSchema,
} from "../../src/gen/auth_pb";
import {
  GenerateTokenResponseSchema,
  RefreshTokenResponseSchema,
} from "../../src/gen/token_pb";

const toArrayBuffer = (u8: Uint8Array) => {
  const buf = new ArrayBuffer(u8.length);
  new Uint8Array(buf).set(u8);
  return buf;
};

/** Binary request body from `cy.intercept` (ArrayBuffer, Buffer, Uint8Array, or binary string). */
function connectRequestBodyToUint8(body: unknown): Uint8Array {
  if (body instanceof Uint8Array) {
    return body;
  }
  if (body instanceof ArrayBuffer) {
    return new Uint8Array(body);
  }
  if (typeof Buffer !== "undefined" && Buffer.isBuffer(body)) {
    return new Uint8Array(body.buffer, body.byteOffset, body.byteLength);
  }
  if (typeof body === "string") {
    const out = new Uint8Array(body.length);
    for (let i = 0; i < body.length; i++) {
      out[i] = body.charCodeAt(i) & 0xff;
    }
    return out;
  }
  throw new Error(`Unsupported request body: ${Object.prototype.toString.call(body)}`);
}

const ACTIVE_SESSION = {
  sessionId: "session-active-1",
  createdAt: "2026-03-21T10:00:00Z",
  status: "active",
  repoPath: "/home/dev/project",
  pid: 12345,
  isActive: true,
  projectId: "proj-1",
};

/** Same project as ACTIVE_SESSION — `pending_elicitation` true (acceptance: row exposes indicator). */
const SESSION_PENDING_ELICITATION = {
  ...ACTIVE_SESSION,
  sessionId: "session-elicitation-pending-1",
  pendingElicitation: true,
};

/** Control row — elicitation explicitly false. */
const SESSION_WITHOUT_ELICITATION = {
  ...ACTIVE_SESSION,
  sessionId: "session-no-elicit-1",
  pendingElicitation: false,
};

const INACTIVE_SESSION = {
  sessionId: "session-inactive-1",
  createdAt: "2026-03-20T10:00:00Z",
  status: "exited",
  repoPath: "/home/dev/project",
  pid: 0,
  isActive: false,
  projectId: "proj-1",
};

/** Project id not in ListProjects — appears under "Other sessions". */
const ORPHAN_ACTIVE_SESSION = {
  sessionId: "orphan-active-1",
  createdAt: "2026-03-21T10:00:00Z",
  status: "active",
  repoPath: "/home/dev/orphan",
  pid: 99901,
  isActive: true,
  projectId: "unknown-project-id",
};

const ORPHAN_INACTIVE_SESSION = {
  sessionId: "orphan-inactive-1",
  createdAt: "2026-03-19T10:00:00Z",
  status: "exited",
  repoPath: "/home/dev/orphan",
  pid: 0,
  isActive: false,
  projectId: "unknown-project-id",
};

/** Project `proj-1` — for ordering specs; API returns this non-canonical order on purpose. */
const PROJ_ORDER_ACTIVE_NEW = {
  sessionId: "proj-order-active-new",
  createdAt: "2026-03-21T12:00:00Z",
  status: "active",
  repoPath: "/home/dev/project",
  pid: 401,
  isActive: true,
  projectId: "proj-1",
};
const PROJ_ORDER_ACTIVE_OLD = {
  sessionId: "proj-order-active-old",
  createdAt: "2026-03-21T08:00:00Z",
  status: "active",
  repoPath: "/home/dev/project",
  pid: 402,
  isActive: true,
  projectId: "proj-1",
};
const PROJ_ORDER_INACTIVE_NEW = {
  sessionId: "proj-order-inactive-new",
  createdAt: "2026-03-21T11:00:00Z",
  status: "exited",
  repoPath: "/home/dev/project",
  pid: 0,
  isActive: false,
  projectId: "proj-1",
};
const PROJ_ORDER_INACTIVE_OLD = {
  sessionId: "proj-order-inactive-old",
  createdAt: "2026-03-21T07:00:00Z",
  status: "exited",
  repoPath: "/home/dev/project",
  pid: 0,
  isActive: false,
  projectId: "proj-1",
};

/** Orphans share a project id that is not in ListProjects. */
const ORPHAN_ORDER_PROJECT_ID = "orphan-order-unknown-pid";

const ORPH_ORDER_ACTIVE_NEW = {
  sessionId: "orph-order-active-new",
  createdAt: "2026-03-21T12:00:00Z",
  status: "active",
  repoPath: "/home/dev/orphan-order",
  pid: 501,
  isActive: true,
  projectId: ORPHAN_ORDER_PROJECT_ID,
};
const ORPH_ORDER_ACTIVE_OLD = {
  sessionId: "orph-order-active-old",
  createdAt: "2026-03-21T08:00:00Z",
  status: "active",
  repoPath: "/home/dev/orphan-order",
  pid: 502,
  isActive: true,
  projectId: ORPHAN_ORDER_PROJECT_ID,
};
const ORPH_ORDER_INACTIVE_NEW = {
  sessionId: "orph-order-inactive-new",
  createdAt: "2026-03-21T11:00:00Z",
  status: "exited",
  repoPath: "/home/dev/orphan-order",
  pid: 0,
  isActive: false,
  projectId: ORPHAN_ORDER_PROJECT_ID,
};
const ORPH_ORDER_INACTIVE_OLD = {
  sessionId: "orph-order-inactive-old",
  createdAt: "2026-03-21T07:00:00Z",
  status: "exited",
  repoPath: "/home/dev/orphan-order",
  pid: 0,
  isActive: false,
  projectId: ORPHAN_ORDER_PROJECT_ID,
};

const PROJECT = {
  projectId: "proj-1",
  name: "Test Project",
  gitUrl: "https://github.com/test/project.git",
  mainRepoPath: "/home/dev/project",
};

function mockAuthAuthenticated() {
  const user = create(GitHubUserSchema, {
    login: "testuser",
    avatarUrl: "https://example.com/avatar.png",
    name: "Test User",
    id: BigInt(42),
  });
  return toBinary(
    GetAuthStatusResponseSchema,
    create(GetAuthStatusResponseSchema, { authenticated: true, user }),
  );
}

function mockListToolsResponse() {
  return toBinary(
    ListToolsResponseSchema,
    create(ListToolsResponseSchema, {
      tools: [
        create(ToolInfoSchema, { path: "/usr/bin/tddy-coder", label: "tddy-coder" }),
      ],
    }),
  );
}

/** Matches default dev.daemon.yaml `allowed_agents`. */
const MOCK_DEFAULT_LIST_AGENTS = [
  { id: "claude", label: "Claude (opus)" },
  { id: "claude-acp", label: "Claude ACP (opus)" },
  { id: "cursor", label: "Cursor (composer-2)" },
  { id: "stub", label: "Stub" },
  { id: "codex", label: "Codex" },
  { id: "codex-acp", label: "Codex ACP" },
];

function mockListAgentsResponse(agents: Array<{ id: string; label: string }>) {
  return toBinary(
    ListAgentsResponseSchema,
    create(ListAgentsResponseSchema, {
      agents: agents.map((a) => create(AgentInfoSchema, { id: a.id, label: a.label })),
    }),
  );
}

type MockSessionRow = {
  sessionId: string;
  createdAt: string;
  status: string;
  repoPath: string;
  pid: number;
  isActive: boolean;
  projectId: string;
  daemonInstanceId?: string;
  workflowGoal?: string;
  workflowState?: string;
  elapsedDisplay?: string;
  agent?: string;
  model?: string;
  pendingElicitation?: boolean;
};

function mockListSessionsResponse(sessions: MockSessionRow[]) {
  return toBinary(
    ListSessionsResponseSchema,
    create(ListSessionsResponseSchema, {
      sessions: sessions.map((s) =>
        create(SessionEntrySchema, {
          ...s,
          pendingElicitation: s.pendingElicitation ?? false,
        }),
      ),
    }),
  );
}

function mockListProjectsResponse() {
  return toBinary(
    ListProjectsResponseSchema,
    create(ListProjectsResponseSchema, {
      projects: [create(ProjectEntrySchema, PROJECT)],
    }),
  );
}

function mockListEligibleDaemonsDefaultResponse() {
  return toBinary(
    ListEligibleDaemonsResponseSchema,
    create(ListEligibleDaemonsResponseSchema, {
      daemons: [
        create(EligibleDaemonEntrySchema, { instanceId: "local", label: "local (this daemon)", isLocal: true }),
      ],
    }),
  );
}

function mockListEligibleDaemonsResponse(
  daemons: Array<{ instanceId: string; label: string; isLocal: boolean }> = [
    { instanceId: "workstation-1", label: "workstation-1 (this daemon)", isLocal: true },
    { instanceId: "server-2", label: "server-2", isLocal: false },
  ],
) {
  return toBinary(
    ListEligibleDaemonsResponseSchema,
    create(ListEligibleDaemonsResponseSchema, {
      daemons: daemons.map((d) => create(EligibleDaemonEntrySchema, d)),
    }),
  );
}

function interceptAllRpcs(
  sessions: MockSessionRow[],
  daemonsOverride?: Array<{
    instanceId: string;
    label: string;
    isLocal: boolean;
  }>,
) {
  const authBody = mockAuthAuthenticated();
  cy.intercept("POST", "**/rpc/auth.AuthService/GetAuthStatus", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(authBody),
    });
  }).as("getAuthStatus");

  const toolsBody = mockListToolsResponse();
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListTools", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(toolsBody),
    });
  }).as("listTools");

  const agentsBody = mockListAgentsResponse(MOCK_DEFAULT_LIST_AGENTS);
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListAgents", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(agentsBody),
    });
  }).as("listAgents");

  const daemonsBody =
    daemonsOverride === undefined
      ? mockListEligibleDaemonsDefaultResponse()
      : mockListEligibleDaemonsResponse(daemonsOverride);
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListEligibleDaemons", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(daemonsBody),
    });
  }).as("listEligibleDaemons");

  const sessionsBody = mockListSessionsResponse(sessions);
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListSessions", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(sessionsBody),
    });
  }).as("listSessions");

  const projectsBody = mockListProjectsResponse();
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListProjects", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(projectsBody),
    });
  }).as("listProjects");
}

function interceptSignalSessionSuccess() {
  // ok=true encoded as protobuf: field 1 (varint), value 1
  const okResponse = new Uint8Array([0x08, 0x01]);
  cy.intercept("POST", "**/rpc/connection.ConnectionService/SignalSession", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(okResponse),
    });
  }).as("signalSession");
}

function interceptSignalSessionError() {
  cy.intercept("POST", "**/rpc/connection.ConnectionService/SignalSession", (req) => {
    req.reply({
      statusCode: 412,
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ code: "failed_precondition", message: "Process not alive" }),
    });
  }).as("signalSessionError");
}

/** DeleteSessionResponse { ok: true } — field 1 varint 1 */
function interceptDeleteSessionSuccess() {
  const okResponse = new Uint8Array([0x08, 0x01]);
  cy.intercept("POST", "**/rpc/connection.ConnectionService/DeleteSession", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(okResponse),
    });
  }).as("deleteSession");
}

function interceptConnectSessionSuccess() {
  const body = toBinary(
    ConnectSessionResponseSchema,
    create(ConnectSessionResponseSchema, {
      livekitRoom: "session-room-ct",
      livekitUrl: "ws://127.0.0.1:7880",
      livekitServerIdentity: "server",
    }),
  );
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ConnectSession", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(body),
    });
  }).as("connectSession");
}

function interceptResumeSessionSuccess(sessionId: string) {
  const body = toBinary(
    ResumeSessionResponseSchema,
    create(ResumeSessionResponseSchema, {
      sessionId,
      livekitRoom: "resume-room-ct",
      livekitUrl: "ws://127.0.0.1:7880",
      livekitServerIdentity: "server",
    }),
  );
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ResumeSession", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(body),
    });
  }).as("resumeSession");
}

function interceptTokenForPresence() {
  const mockToken = "mock-jwt-presence";
  const mockTtl = 600;
  const generateBody = toBinary(
    GenerateTokenResponseSchema,
    create(GenerateTokenResponseSchema, {
      token: mockToken,
      ttlSeconds: BigInt(mockTtl),
    }),
  );
  const refreshBody = toBinary(
    RefreshTokenResponseSchema,
    create(RefreshTokenResponseSchema, {
      token: mockToken,
      ttlSeconds: BigInt(mockTtl),
    }),
  );
  cy.intercept("POST", "**/rpc/token.TokenService/GenerateToken", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(generateBody),
    });
  }).as("generateTokenPresence");
  cy.intercept("POST", "**/rpc/token.TokenService/RefreshToken", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(refreshBody),
    });
  }).as("refreshTokenPresence");
}

describe("ConnectionScreen terminal chrome — status dot menu", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
  });

  it("ConnectionScreen connected daemon session exposes Terminate from the dot menu", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcs([ACTIVE_SESSION]);
    interceptConnectSessionSuccess();
    interceptTokenForPresence();
    interceptSignalSessionSuccess();
    cy.window().then((win) => {
      cy.stub(win, "confirm").returns(true).as("confirmTerminate");
    });
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    cy.get(`[data-testid="connect-${ACTIVE_SESSION.sessionId}"]`, { timeout: 5000 }).click();
    cy.wait("@connectSession");
    cy.get("[data-testid='connected-terminal-container']", { timeout: 5000 }).should("exist");
    // Token fetch uses standalone chrome; after the JWT arrives Ghostty mounts and replaces the tree — wait for LiveKit UI before clicking the dot (avoids detached DOM).
    cy.get("[data-testid='connection-status-dot']", { timeout: 20000 })
      .should("be.visible")
      .and("have.attr", "data-connection-status");
    cy.get("[data-testid='livekit-status']").should("not.be.visible");
    cy.get("[data-testid='connection-status-dot']", { timeout: 5000 }).should("be.visible").click();
    cy.get("[data-testid='connection-menu-disconnect']", { timeout: 10000 }).should("be.visible");
    cy.get("[data-testid='connection-menu-terminate']", { timeout: 10000 }).should("be.visible");
    cy.get("[data-testid='connection-menu-terminate']").click({ force: true });
    cy.get("@confirmTerminate").should("have.been.calledOnce");
    cy.wait("@signalSession").its("request.url").should("include", "SignalSession");
  });

  it("cancelling Terminate confirmation does not call SignalSession", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcs([ACTIVE_SESSION]);
    interceptConnectSessionSuccess();
    interceptTokenForPresence();
    let signalSessionRequests = 0;
    const okResponse = new Uint8Array([0x08, 0x01]);
    cy.intercept("POST", "**/rpc/connection.ConnectionService/SignalSession", (req) => {
      signalSessionRequests += 1;
      req.reply({
        statusCode: 200,
        headers: { "Content-Type": "application/proto" },
        body: toArrayBuffer(okResponse),
      });
    }).as("signalSessionTerminateCancel");
    cy.window().then((win) => {
      cy.stub(win, "confirm").returns(false);
    });
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    cy.get(`[data-testid="connect-${ACTIVE_SESSION.sessionId}"]`, { timeout: 5000 }).click();
    cy.wait("@connectSession");
    cy.get("[data-testid='connected-terminal-container']", { timeout: 5000 }).should("exist");
    // Match the stable LiveKit chrome wait from the Terminate test — avoids detached DOM during async remount.
    cy.get("[data-testid='connection-status-dot']", { timeout: 20000 })
      .should("be.visible")
      .and("have.attr", "data-connection-status");
    cy.get("[data-testid='livekit-status']").should("not.be.visible");
    cy.get("[data-testid='connection-status-dot']").should("be.visible").click();
    cy.get("[data-testid='connection-menu-disconnect']", { timeout: 10000 }).should("be.visible");
    cy.get("[data-testid='connection-menu-terminate']", { timeout: 10000 }).should("be.visible");
    cy.get("[data-testid='connection-menu-terminate']").click({ force: true });
    cy.then(() => {
      expect(signalSessionRequests, "Terminate cancel must not hit SignalSession").to.equal(0);
    });
  });
});

describe("ConnectionScreen connected participants", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
  });

  it("does not render presence panel without common room config", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcs([ACTIVE_SESSION]);
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    cy.get("[data-testid='sessions-table-proj-1']", { timeout: 5000 }).should("exist");
    cy.get("[data-testid='connected-participants-panel']").should("not.exist");
  });

  it("renders presence panel when livekit URL and common room are provided", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcs([ACTIVE_SESSION]);
    interceptTokenForPresence();
    cy.mount(
      <ConnectionScreen livekitUrl="ws://127.0.0.1:7880" commonRoom="tddy-lobby" />,
    );
    cy.wait("@getAuthStatus");
    cy.get("[data-testid='connected-participants-panel']", { timeout: 5000 }).should("exist");
    cy.get("[data-testid='participant-list']", { timeout: 5000 }).should("exist");
    cy.wait("@generateTokenPresence");
    cy.get("[data-testid='participant-list']", { timeout: 20000 }).should(($el) => {
      const status = $el.attr("data-room-status");
      expect(status === "error" || status === "connected" || status === "connecting").to.be.true;
    });
  });
});

describe("ConnectionScreen Signal Dropdown", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
  });

  it("renders signal dropdown for active sessions", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcs([ACTIVE_SESSION]);
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    cy.get(`[data-testid="signal-dropdown-${ACTIVE_SESSION.sessionId}"]`, { timeout: 5000 })
      .should("exist");
  });

  it("hides signal dropdown for inactive sessions", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcs([ACTIVE_SESSION, INACTIVE_SESSION]);
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    cy.get(`[data-testid="sessions-table-${PROJECT.projectId}"]`, { timeout: 5000 })
      .should("exist");
    cy.get(`[data-testid="signal-dropdown-${ACTIVE_SESSION.sessionId}"]`)
      .should("exist");
    cy.get(`[data-testid="signal-dropdown-${INACTIVE_SESSION.sessionId}"]`)
      .should("not.exist");
  });

  it("signal dropdown shows three options", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcs([ACTIVE_SESSION]);
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    cy.get(`[data-testid="signal-dropdown-${ACTIVE_SESSION.sessionId}"]`, { timeout: 5000 })
      .click();
    cy.get(`[data-testid="signal-sigint-${ACTIVE_SESSION.sessionId}"]`)
      .should("exist")
      .and("contain.text", "Interrupt");
    cy.get(`[data-testid="signal-sigterm-${ACTIVE_SESSION.sessionId}"]`)
      .should("exist")
      .and("contain.text", "Terminate");
    cy.get(`[data-testid="signal-sigkill-${ACTIVE_SESSION.sessionId}"]`)
      .should("exist")
      .and("contain.text", "Kill");
  });

  it("clicking interrupt calls signal session rpc with sigint", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcs([ACTIVE_SESSION]);
    interceptSignalSessionSuccess();
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    cy.get(`[data-testid="signal-dropdown-${ACTIVE_SESSION.sessionId}"]`, { timeout: 5000 })
      .click();
    cy.get(`[data-testid="signal-sigint-${ACTIVE_SESSION.sessionId}"]`).click();
    cy.wait("@signalSession").its("request.url").should("include", "SignalSession");
  });

  it("clicking terminate calls signal session rpc with sigterm", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcs([ACTIVE_SESSION]);
    interceptSignalSessionSuccess();
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    cy.get(`[data-testid="signal-dropdown-${ACTIVE_SESSION.sessionId}"]`, { timeout: 5000 })
      .click();
    cy.get(`[data-testid="signal-sigterm-${ACTIVE_SESSION.sessionId}"]`).click();
    cy.wait("@signalSession").its("request.url").should("include", "SignalSession");
  });

  it("clicking kill calls signal session rpc with sigkill", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcs([ACTIVE_SESSION]);
    interceptSignalSessionSuccess();
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    cy.get(`[data-testid="signal-dropdown-${ACTIVE_SESSION.sessionId}"]`, { timeout: 5000 })
      .click();
    cy.get(`[data-testid="signal-sigkill-${ACTIVE_SESSION.sessionId}"]`).click();
    cy.wait("@signalSession").its("request.url").should("include", "SignalSession");
  });

  it("dropdown closes after action", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcs([ACTIVE_SESSION]);
    interceptSignalSessionSuccess();
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    cy.get(`[data-testid="signal-dropdown-${ACTIVE_SESSION.sessionId}"]`, { timeout: 5000 })
      .click();
    cy.get(`[data-testid="signal-menu-${ACTIVE_SESSION.sessionId}"]`)
      .should("exist");
    cy.get(`[data-testid="signal-sigint-${ACTIVE_SESSION.sessionId}"]`).click();
    cy.get(`[data-testid="signal-menu-${ACTIVE_SESSION.sessionId}"]`)
      .should("not.exist");
  });

  it("shows error when signal rpc fails", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcs([ACTIVE_SESSION]);
    interceptSignalSessionError();
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    cy.get(`[data-testid="signal-dropdown-${ACTIVE_SESSION.sessionId}"]`, { timeout: 5000 })
      .click();
    cy.get(`[data-testid="signal-sigint-${ACTIVE_SESSION.sessionId}"]`).click();
    cy.get("[data-testid='connection-error']", { timeout: 5000 })
      .should("exist");
  });
});

describe("ConnectionScreen Delete session", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
  });

  const sessionsForDeleteSuite = [
    ACTIVE_SESSION,
    INACTIVE_SESSION,
    ORPHAN_ACTIVE_SESSION,
    ORPHAN_INACTIVE_SESSION,
  ];

  it("delete_button_visible_for_active_and_inactive_session_rows", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcs(sessionsForDeleteSuite);
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    cy.get(`[data-testid="sessions-table-${PROJECT.projectId}"]`, { timeout: 5000 }).should("exist");
    cy.get(`[data-testid="connect-${ACTIVE_SESSION.sessionId}"]`, { timeout: 5000 }).should("exist");
    cy.get(`[data-testid="connect-${ORPHAN_ACTIVE_SESSION.sessionId}"]`).should("exist");
    cy.get(`[data-testid="delete-session-${ACTIVE_SESSION.sessionId}"]`).should("exist");
    cy.get(`[data-testid="delete-session-${ORPHAN_ACTIVE_SESSION.sessionId}"]`).should("exist");
    cy.get(`[data-testid="delete-session-${INACTIVE_SESSION.sessionId}"]`).should("exist");
    cy.get(`[data-testid="delete-session-${ORPHAN_INACTIVE_SESSION.sessionId}"]`).should("exist");
    cy.get("[data-testid='sessions-table-orphan']", { timeout: 5000 }).should("exist");
  });

  it("clicking_delete_confirmed_calls_delete_session_rpc", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcs(sessionsForDeleteSuite);
    interceptDeleteSessionSuccess();
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    cy.window().then((win) => {
      cy.stub(win, "confirm").returns(true);
    });
    cy.get(`[data-testid="delete-session-${INACTIVE_SESSION.sessionId}"]`, { timeout: 5000 })
      .click();
    cy.wait("@deleteSession").then((interception) => {
      expect(interception.request.url).to.include("DeleteSession");
    });
  });
});

/** Bulk selection / delete acceptance: same RPC wiring as `interceptAllRpcs` but tracks DeleteSession bodies. */
function interceptAllRpcsWithTrackedDelete(
  sessions: MockSessionRow[],
  options?: { captureDeleteSessionIds?: string[] },
) {
  const authBody = mockAuthAuthenticated();
  cy.intercept("POST", "**/rpc/auth.AuthService/GetAuthStatus", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(authBody),
    });
  }).as("getAuthStatus");

  const toolsBody = mockListToolsResponse();
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListTools", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(toolsBody),
    });
  }).as("listTools");

  const agentsBody = mockListAgentsResponse(MOCK_DEFAULT_LIST_AGENTS);
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListAgents", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(agentsBody),
    });
  }).as("listAgents");

  const daemonsBody = mockListEligibleDaemonsDefaultResponse();
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListEligibleDaemons", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(daemonsBody),
    });
  }).as("listEligibleDaemons");

  const sessionsBody = mockListSessionsResponse(sessions);
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListSessions", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(sessionsBody),
    });
  }).as("listSessions");

  const projectsBody = mockListProjectsResponse();
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListProjects", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(projectsBody),
    });
  }).as("listProjects");

  const okDelete = new Uint8Array([0x08, 0x01]);
  cy.intercept("POST", "**/rpc/connection.ConnectionService/DeleteSession", (req) => {
    const u8 = connectRequestBodyToUint8(req.body);
    const decoded = fromBinary(DeleteSessionRequestSchema, u8);
    options?.captureDeleteSessionIds?.push(decoded.sessionId);
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(okDelete),
    });
  }).as("deleteSession");
}

describe("ConnectionScreen bulk session selection and delete (acceptance)", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
  });

  it("header select all checks every row checkbox in a project session table", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcs([ACTIVE_SESSION, INACTIVE_SESSION]);
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    const tableSel = `[data-testid="sessions-table-${PROJECT.projectId}"]`;
    cy.get(`${tableSel} [data-testid="session-table-select-all-${PROJECT.projectId}"]`, {
      timeout: 5000,
    }).click();
    cy.get(`${tableSel} [data-testid="session-row-select-${ACTIVE_SESSION.sessionId}"]`).should(
      "be.checked",
    );
    cy.get(`${tableSel} [data-testid="session-row-select-${INACTIVE_SESSION.sessionId}"]`).should(
      "be.checked",
    );
    cy.get(`${tableSel} [data-testid="session-table-select-all-${PROJECT.projectId}"]`).should(
      "be.checked",
    );
  });

  it("header checkbox shows indeterminate when selection is partial", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcs([ACTIVE_SESSION, INACTIVE_SESSION]);
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    const tableSel = `[data-testid="sessions-table-${PROJECT.projectId}"]`;
    cy.get(`${tableSel} [data-testid="session-row-select-${ACTIVE_SESSION.sessionId}"]`, {
      timeout: 5000,
    }).click();
    cy.get(`${tableSel} [data-testid="session-row-select-${INACTIVE_SESSION.sessionId}"]`).should(
      "not.be.checked",
    );
    cy.get(`${tableSel} [data-testid="session-table-select-all-${PROJECT.projectId}"]`).should(
      ($el) => {
        const el = $el.get(0) as HTMLInputElement;
        expect(el.indeterminate, "header checkbox indeterminate when partial").to.be.true;
      },
    );
  });

  it("bulk delete confirms once and calls deleteSession once per selected id", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    const capturedDeleteSessionIds: string[] = [];
    interceptAllRpcsWithTrackedDelete([ACTIVE_SESSION, INACTIVE_SESSION], {
      captureDeleteSessionIds: capturedDeleteSessionIds,
    });
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    const tableSel = `[data-testid="sessions-table-${PROJECT.projectId}"]`;
    cy.get(`${tableSel} [data-testid="session-table-select-all-${PROJECT.projectId}"]`, {
      timeout: 5000,
    }).click();
    cy.window().then((win) => {
      cy.stub(win, "confirm")
        .callsFake((msg: string) => {
          expect(msg).to.include("2");
          expect(msg.toLowerCase()).to.include("delete");
          return true;
        })
        .as("bulkDeleteConfirm");
    });
    cy.get(`${tableSel} [data-testid="bulk-delete-selected-${PROJECT.projectId}"]`).click();
    cy.get("@bulkDeleteConfirm").should("have.been.calledOnce");
    cy.wait("@deleteSession");
    cy.wait("@deleteSession");
    cy.then(() => {
      expect(capturedDeleteSessionIds.slice().sort()).to.deep.equal(
        [ACTIVE_SESSION.sessionId, INACTIVE_SESSION.sessionId].sort(),
      );
    });
    cy.get("@listSessions.all").should("have.length.at.least", 2);
  });

  it("bulk delete does not call DeleteSession when user cancels confirm", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcs([ACTIVE_SESSION, INACTIVE_SESSION]);
    let deleteSessionRequests = 0;
    const okDelete = new Uint8Array([0x08, 0x01]);
    cy.intercept("POST", "**/rpc/connection.ConnectionService/DeleteSession", (req) => {
      deleteSessionRequests += 1;
      req.reply({
        statusCode: 200,
        headers: { "Content-Type": "application/proto" },
        body: toArrayBuffer(okDelete),
      });
    });
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    const tableSel = `[data-testid="sessions-table-${PROJECT.projectId}"]`;
    cy.get(`${tableSel} [data-testid="session-table-select-all-${PROJECT.projectId}"]`, {
      timeout: 5000,
    }).click();
    cy.window().then((win) => {
      cy.stub(win, "confirm").returns(false);
    });
    cy.get(`${tableSel} [data-testid="bulk-delete-selected-${PROJECT.projectId}"]`).click();
    cy.then(() => {
      expect(deleteSessionRequests, "cancelled bulk delete must not call DeleteSession").to.equal(0);
    });
  });

  it("Delete selected is disabled when no rows are selected", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcs([ACTIVE_SESSION, INACTIVE_SESSION]);
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    cy.get(`[data-testid="bulk-delete-selected-${PROJECT.projectId}"]`, { timeout: 5000 }).should(
      "be.disabled",
    );
  });

  it("orphan table bulk selection does not clear project table selection", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcs([ACTIVE_SESSION, INACTIVE_SESSION, ORPHAN_ACTIVE_SESSION]);
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    const projTable = `[data-testid="sessions-table-${PROJECT.projectId}"]`;
    const orphanTable = `[data-testid="sessions-table-orphan"]`;
    cy.get(`${projTable} [data-testid="session-table-select-all-${PROJECT.projectId}"]`, {
      timeout: 5000,
    }).click();
    cy.get(`${orphanTable} [data-testid="session-row-select-${ORPHAN_ACTIVE_SESSION.sessionId}"]`)
      .click();
    cy.get(`${projTable} [data-testid="session-row-select-${ACTIVE_SESSION.sessionId}"]`).should(
      "be.checked",
    );
    cy.get(`${projTable} [data-testid="session-row-select-${INACTIVE_SESSION.sessionId}"]`).should(
      "be.checked",
    );
    cy.get(`${orphanTable} [data-testid="session-row-select-${ORPHAN_ACTIVE_SESSION.sessionId}"]`).should(
      "be.checked",
    );
  });
});

describe("ConnectionScreen session table ordering", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
  });

  it("ConnectionScreen — sorts project sessions with active on top then by createdAt descending", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    /** Deliberately wrong: inactive rows first; active newest last — sorted UI must fix order. */
    const wrongApiOrder = [
      PROJ_ORDER_INACTIVE_OLD,
      PROJ_ORDER_ACTIVE_OLD,
      PROJ_ORDER_INACTIVE_NEW,
      PROJ_ORDER_ACTIVE_NEW,
    ];
    interceptAllRpcs(wrongApiOrder);
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    const tableSel = `[data-testid="sessions-table-${PROJECT.projectId}"]`;
    cy.get(tableSel, { timeout: 5000 }).should("exist");
    const expectedOrder = [
      PROJ_ORDER_ACTIVE_NEW.sessionId,
      PROJ_ORDER_ACTIVE_OLD.sessionId,
      PROJ_ORDER_INACTIVE_NEW.sessionId,
      PROJ_ORDER_INACTIVE_OLD.sessionId,
    ];
    expectedOrder.forEach((sessionId, index) => {
      cy.get(`${tableSel} tbody tr`)
        .eq(index)
        .find(`[data-testid="connect-${sessionId}"], [data-testid="resume-${sessionId}"]`)
        .should("exist");
    });
  });

  it("ConnectionScreen — sorts orphan sessions with active on top then by createdAt descending", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    const wrongApiOrder = [
      ORPH_ORDER_INACTIVE_OLD,
      ORPH_ORDER_ACTIVE_OLD,
      ORPH_ORDER_INACTIVE_NEW,
      ORPH_ORDER_ACTIVE_NEW,
    ];
    interceptAllRpcs(wrongApiOrder);
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    cy.get("[data-testid='sessions-table-orphan']", { timeout: 5000 }).should("exist");
    const expectedOrder = [
      ORPH_ORDER_ACTIVE_NEW.sessionId,
      ORPH_ORDER_ACTIVE_OLD.sessionId,
      ORPH_ORDER_INACTIVE_NEW.sessionId,
      ORPH_ORDER_INACTIVE_OLD.sessionId,
    ];
    const tableSel = "[data-testid='sessions-table-orphan']";
    expectedOrder.forEach((sessionId, index) => {
      cy.get(`${tableSel} tbody tr`)
        .eq(index)
        .find(`[data-testid="connect-${sessionId}"], [data-testid="resume-${sessionId}"]`)
        .should("exist");
    });
  });
});

/** Extended ListSessions payload — acceptance / TUI parity (workflow columns). */
const STATUS_PARITY_SESSION_V1: MockSessionRow = {
  sessionId: "session-status-parity-1",
  createdAt: "2026-03-21T10:00:00Z",
  status: "active",
  repoPath: "/home/dev/project",
  pid: 12345,
  isActive: true,
  projectId: "proj-1",
  daemonInstanceId: "workstation-1",
  workflowGoal: "acceptance-tests",
  workflowState: "Red",
  elapsedDisplay: "1m 2s",
  agent: "claude",
  model: "sonnet-4",
};

const STATUS_PARITY_SESSION_V2: MockSessionRow = {
  ...STATUS_PARITY_SESSION_V1,
  workflowState: "Green",
  elapsedDisplay: "3m 0s",
};

function interceptAllRpcsWithListSessionsFactory(getSessions: () => MockSessionRow[]) {
  const authBody = mockAuthAuthenticated();
  cy.intercept("POST", "**/rpc/auth.AuthService/GetAuthStatus", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(authBody),
    });
  }).as("getAuthStatus");

  const toolsBody = mockListToolsResponse();
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListTools", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(toolsBody),
    });
  }).as("listTools");

  const agentsBody = mockListAgentsResponse(MOCK_DEFAULT_LIST_AGENTS);
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListAgents", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(agentsBody),
    });
  }).as("listAgents");

  const daemonsBody = mockListEligibleDaemonsDefaultResponse();
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListEligibleDaemons", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(daemonsBody),
    });
  }).as("listEligibleDaemons");

  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListSessions", (req) => {
    const sessionsBody = mockListSessionsResponse(getSessions());
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(sessionsBody),
    });
  }).as("listSessions");

  const projectsBody = mockListProjectsResponse();
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListProjects", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(projectsBody),
    });
  }).as("listProjects");
}

// ---------------------------------------------------------------------------
// Multi-host daemon selection acceptance tests
// ---------------------------------------------------------------------------

const DAEMON_LOCAL = { instanceId: "workstation-1", label: "workstation-1 (this daemon)", isLocal: true };
const DAEMON_PEER = { instanceId: "server-2", label: "server-2", isLocal: false };

const SESSION_WITH_HOST: MockSessionRow = {
  sessionId: "session-host-1",
  createdAt: "2026-03-28T10:00:00Z",
  status: "active",
  repoPath: "/home/dev/project",
  pid: 12345,
  isActive: true,
  projectId: "proj-1",
  daemonInstanceId: "workstation-1",
};

function interceptAllRpcsWithDaemons(
  sessions: MockSessionRow[],
  daemons: Array<{ instanceId: string; label: string; isLocal: boolean }> = [DAEMON_LOCAL, DAEMON_PEER],
) {
  interceptAllRpcs(sessions, daemons);
}

describe("ConnectionScreen session status (TUI parity)", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
  });

  it("connection_screen_renders_status_columns_from_extended_session_entry", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcs([STATUS_PARITY_SESSION_V1]);
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    const sid = STATUS_PARITY_SESSION_V1.sessionId;
    cy.get(`[data-testid="session-row-workflow-goal-${sid}"]`, { timeout: 8000 })
      .scrollIntoView()
      .should("be.visible")
      .and("contain.text", "acceptance-tests");
    cy.get(`[data-testid="session-row-workflow-state-${sid}"]`)
      .should("contain.text", "Red");
    cy.get(`[data-testid="session-row-agent-${sid}"]`).should("contain.text", "claude");
    cy.get(`[data-testid="session-row-model-${sid}"]`).should("contain.text", "sonnet-4");
  });

  it("connection_screen_updates_row_when_live_status_payload_changes", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    let call = 0;
    interceptAllRpcsWithListSessionsFactory(() => {
      call += 1;
      return [call === 1 ? STATUS_PARITY_SESSION_V1 : STATUS_PARITY_SESSION_V2];
    });
    cy.mount(<ConnectionScreen />);
    cy.wait("@listSessions");
    const sid = STATUS_PARITY_SESSION_V1.sessionId;
    cy.get(`[data-testid="session-row-workflow-state-${sid}"]`, { timeout: 8000 }).should(
      "contain.text",
      "Red",
    );
    cy.wait(5500);
    cy.wait("@listSessions");
    cy.get(`[data-testid="session-row-workflow-state-${sid}"]`).should("contain.text", "Green");
    cy.get(`[data-testid="session-row-elapsed-${sid}"]`).should("contain.text", "3m 0s");
  });
});

/** Same as `interceptAllRpcs` but overrides the `ListAgents` mock payload (acceptance: dynamic backend options). */
function interceptAllRpcsWithListAgents(
  sessions: MockSessionRow[],
  listAgents: Array<{ id: string; label: string }>,
  daemonsOverride?: Array<{
    instanceId: string;
    label: string;
    isLocal: boolean;
  }>,
) {
  const authBody = mockAuthAuthenticated();
  cy.intercept("POST", "**/rpc/auth.AuthService/GetAuthStatus", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(authBody),
    });
  }).as("getAuthStatus");

  const toolsBody = mockListToolsResponse();
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListTools", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(toolsBody),
    });
  }).as("listTools");

  const agentsBody = mockListAgentsResponse(listAgents);
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListAgents", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(agentsBody),
    });
  }).as("listAgents");

  const daemonsBody =
    daemonsOverride === undefined
      ? mockListEligibleDaemonsDefaultResponse()
      : mockListEligibleDaemonsResponse(daemonsOverride);
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListEligibleDaemons", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(daemonsBody),
    });
  }).as("listEligibleDaemons");

  const sessionsBody = mockListSessionsResponse(sessions);
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListSessions", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(sessionsBody),
    });
  }).as("listSessions");

  const projectsBody = mockListProjectsResponse();
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ListProjects", (req) => {
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(projectsBody),
    });
  }).as("listProjects");
}

describe("ConnectionScreen ListAgents backend select (acceptance)", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
  });

  it("connection_screen_backend_select_uses_list_agents", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcsWithListAgents(
      [],
      [
        { id: "agent-one", label: "Agent One" },
        { id: "agent-two", label: "Agent Two" },
      ],
    );
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    cy.wait("@listAgents");
    const backendSel = `[data-testid="backend-select-${PROJECT.projectId}"]`;
    cy.get(backendSel, { timeout: 8000 }).should("exist");
    cy.get(`${backendSel} option`).should("have.length", 2);
    cy.get(`${backendSel} option`).eq(0).should("have.value", "agent-one");
    cy.get(`${backendSel} option`).eq(1).should("have.value", "agent-two");
    cy.get(`${backendSel} option[value="claude"]`).should("not.exist");
    cy.get(`${backendSel} option[value="cursor"]`).should("not.exist");

    interceptAllRpcsWithListAgents([], [{ id: "gamma-only", label: "Gamma" }]);
    cy.mount(<ConnectionScreen />);
    cy.wait("@listAgents");
    cy.get(`[data-testid="backend-select-${PROJECT.projectId}"] option`).should("have.length", 1);
    cy.get(`[data-testid="backend-select-${PROJECT.projectId}"]`)
      .should("have.value", "gamma-only");
  });
});

describe("ConnectionScreen multi-host daemon selection", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
  });

  it("renders a Host dropdown per project populated from ListEligibleDaemons", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcsWithDaemons([ACTIVE_SESSION]);
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    cy.get(`[data-testid="host-select-${PROJECT.projectId}"]`, { timeout: 5000 }).should("exist");
    cy.get(`[data-testid="host-select-${PROJECT.projectId}"] option`).should("have.length", 2);
    cy.get(`[data-testid="host-select-${PROJECT.projectId}"] option`).eq(0).should("contain.text", DAEMON_LOCAL.label);
    cy.get(`[data-testid="host-select-${PROJECT.projectId}"] option`).eq(1).should("contain.text", DAEMON_PEER.label);
  });

  it("defaults Host dropdown to the local daemon", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcsWithDaemons([ACTIVE_SESSION]);
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    cy.get(`[data-testid="host-select-${PROJECT.projectId}"]`, { timeout: 5000 })
      .should("have.value", DAEMON_LOCAL.instanceId);
  });

  it("web_start_session_includes_recipe_field", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcsWithDaemons([]);

    cy.intercept("POST", "**/rpc/connection.ConnectionService/StartSession", (req) => {
      req.reply({
        statusCode: 200,
        headers: { "Content-Type": "application/proto" },
        body: toArrayBuffer(new Uint8Array(0)),
      });
    }).as("startSession");

    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");

    cy.get(`[data-testid="recipe-select-${PROJECT.projectId}"]`, { timeout: 5000 })
      .select("bugfix");
    cy.get(`[data-testid="start-session-${PROJECT.projectId}"]`).click();

    cy.wait("@startSession").then((interception) => {
      const bodyBytes = new Uint8Array(interception.request.body as ArrayBuffer);
      const decoded = fromBinary(StartSessionRequestSchema, bodyBytes);
      expect(decoded.recipe).to.eq("bugfix");
    });
  });

  it("sends daemonInstanceId in StartSession request", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcsWithDaemons([]);

    cy.intercept("POST", "**/rpc/connection.ConnectionService/StartSession", (req) => {
      req.reply({
        statusCode: 200,
        headers: { "Content-Type": "application/proto" },
        body: toArrayBuffer(new Uint8Array(0)),
      });
    }).as("startSession");

    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");

    cy.get(`[data-testid="host-select-${PROJECT.projectId}"]`, { timeout: 5000 })
      .select(DAEMON_PEER.instanceId);
    cy.get(`[data-testid="start-session-${PROJECT.projectId}"]`).click();

    cy.wait("@startSession").then((interception) => {
      const bodyBytes = new Uint8Array(interception.request.body as ArrayBuffer);
      const decoded = fromBinary(StartSessionRequestSchema, bodyBytes);
      expect(decoded.daemonInstanceId).to.eq(DAEMON_PEER.instanceId);
    });
  });

  it("shows Host column in session tables", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcsWithDaemons([SESSION_WITH_HOST]);
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    cy.get(`[data-testid="sessions-table-${PROJECT.projectId}"]`, { timeout: 5000 })
      .find("th")
      .should("contain.text", "Host");
    cy.get(`[data-testid="sessions-table-${PROJECT.projectId}"] tbody tr`)
      .first()
      .should("contain.text", "workstation-1");
  });

  it("renders single option when only one daemon is available", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcsWithDaemons([ACTIVE_SESSION], [DAEMON_LOCAL]);
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    cy.get(`[data-testid="host-select-${PROJECT.projectId}"]`, { timeout: 5000 }).should("exist");
    cy.get(`[data-testid="host-select-${PROJECT.projectId}"] option`).should("have.length", 1);
  });
});

describe("ConnectionScreen — reconnect vs new-session presentation", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
  });

  it("Resume inactive session shows floating overlay without pushing /terminal onto history", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcs([INACTIVE_SESSION]);
    interceptResumeSessionSuccess(INACTIVE_SESSION.sessionId);
    interceptTokenForPresence();
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    cy.window().then((win) => {
      cy.spy(win.history, "pushState").as("historyPush");
    });
    cy.get(`[data-testid="resume-${INACTIVE_SESSION.sessionId}"]`, { timeout: 5000 }).click();
    cy.wait("@resumeSession");
    cy.get("[data-testid='terminal-reconnect-overlay-root']", { timeout: 15000 }).should("be.visible");
    cy.get("@historyPush").should("not.have.been.called");
  });

  it("Connect active session opens floating overlay without pushing /terminal onto history", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcs([ACTIVE_SESSION]);
    interceptConnectSessionSuccess();
    interceptTokenForPresence();
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    cy.window().then((win) => {
      cy.spy(win.history, "pushState").as("historyPush");
    });
    cy.get(`[data-testid="connect-${ACTIVE_SESSION.sessionId}"]`, { timeout: 5000 }).click();
    cy.wait("@connectSession");
    cy.get("[data-testid='terminal-reconnect-overlay-root']", { timeout: 15000 }).should("be.visible");
    cy.get("[data-testid='connected-terminal-container']", { timeout: 15000 }).should("exist");
    cy.get("@historyPush").should("not.have.been.called");
  });
});

describe("ConnectionScreen — pending elicitation indicator", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
  });

  it("ConnectionScreen_shows_elicitation_indicator_when_pending_elicitation_true", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcs([SESSION_PENDING_ELICITATION]);
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    cy.get(`[data-testid="sessions-table-${PROJECT.projectId}"]`, { timeout: 5000 }).should("exist");
    cy.get(`[data-testid="connect-${SESSION_PENDING_ELICITATION.sessionId}"]`)
      .closest("tr")
      .should("have.attr", "data-pending-elicitation", "true");
  });

  it("ConnectionScreen_hides_elicitation_indicator_when_pending_elicitation_false", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcs([SESSION_WITHOUT_ELICITATION]);
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    cy.get(`[data-testid="connect-${SESSION_WITHOUT_ELICITATION.sessionId}"]`)
      .closest("tr")
      .should("have.attr", "data-pending-elicitation", "false");
  });
});

// ---------------------------------------------------------------------------
// Multi-session concurrent attachments (acceptance — PRD: Connection screen N≥1)
// ---------------------------------------------------------------------------

const SESSION_MULTI_A: MockSessionRow = {
  sessionId: "multi-session-aaaaaaaa-bbbb-cccc-dddd-eeee11111111",
  createdAt: "2026-03-21T10:00:00Z",
  status: "active",
  repoPath: "/home/dev/project",
  pid: 70001,
  isActive: true,
  projectId: "proj-1",
};

const SESSION_MULTI_B: MockSessionRow = {
  sessionId: "multi-session-bbbbbbbb-aaaa-cccc-dddd-ffff22222222",
  createdAt: "2026-03-21T11:00:00Z",
  status: "active",
  repoPath: "/home/dev/project",
  pid: 70002,
  isActive: true,
  projectId: "proj-1",
};

/** Per-request ConnectSession: room name derived from requested session id (distinct LiveKit rooms). */
function interceptConnectSessionPerSessionId() {
  cy.intercept("POST", "**/rpc/connection.ConnectionService/ConnectSession", (req) => {
    const bodyBytes = new Uint8Array(req.body as ArrayBuffer);
    const decoded = fromBinary(ConnectSessionRequestSchema, bodyBytes);
    const sid = decoded.sessionId;
    const body = toBinary(
      ConnectSessionResponseSchema,
      create(ConnectSessionResponseSchema, {
        livekitRoom: `room-${sid}`,
        livekitUrl: "ws://127.0.0.1:7880",
        livekitServerIdentity: `server-${sid.slice(0, 8)}`,
      }),
    );
    req.reply({
      statusCode: 200,
      headers: { "Content-Type": "application/proto" },
      body: toArrayBuffer(body),
    });
  }).as("connectSessionMulti");
}

describe("ConnectionScreen — multi-session attachments (acceptance)", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
  });

  it("ConnectionScreen — two concurrent connects: two distinct attachment roots (mocked RPC)", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcs([SESSION_MULTI_A, SESSION_MULTI_B]);
    interceptConnectSessionPerSessionId();
    interceptTokenForPresence();
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    cy.get(`[data-testid="connect-${SESSION_MULTI_A.sessionId}"]`, { timeout: 8000 }).click();
    cy.wait("@connectSessionMulti");
    cy.get("[data-testid='connected-terminal-container']", { timeout: 15000 }).should("exist");
    cy.get(`[data-testid="connect-${SESSION_MULTI_B.sessionId}"]`).click();
    cy.wait("@connectSessionMulti");
    cy.get(`[data-testid="connection-attached-terminal-${SESSION_MULTI_A.sessionId}"]`, {
      timeout: 15000,
    }).should("exist");
    cy.get(`[data-testid="connection-attached-terminal-${SESSION_MULTI_B.sessionId}"]`).should(
      "exist",
    );
  });

  it("ConnectionScreen — disconnect first leaves second terminal (mocked)", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcs([SESSION_MULTI_A, SESSION_MULTI_B]);
    interceptConnectSessionPerSessionId();
    interceptTokenForPresence();
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    cy.get(`[data-testid="connect-${SESSION_MULTI_A.sessionId}"]`, { timeout: 8000 }).click();
    cy.wait("@connectSessionMulti");
    cy.get(`[data-testid="connect-${SESSION_MULTI_B.sessionId}"]`).click();
    cy.wait("@connectSessionMulti");
    cy.get(`[data-testid="connection-attached-terminal-${SESSION_MULTI_A.sessionId}"]`, {
      timeout: 15000,
    })
      .find("[data-testid='connection-status-dot']", { timeout: 20000 })
      .should("be.visible")
      .click();
    cy.get("[data-testid='connection-menu-disconnect']", { timeout: 10000 })
      .should("be.visible")
      .click({ force: true });
    cy.get(`[data-testid="connection-attached-terminal-${SESSION_MULTI_B.sessionId}"]`, {
      timeout: 15000,
    }).should("exist");
    cy.get("[data-testid='connection-status-dot']", { timeout: 20000 }).should("exist");
  });

  it("ConnectionScreen — Start New Session with another terminal open adds attachment (mocked)", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcs([SESSION_MULTI_A, SESSION_MULTI_B]);
    interceptConnectSessionPerSessionId();
    const newSessionId = "multi-session-new-start-cccc-dddd-eeee-ffffffffffff";
    cy.intercept("POST", "**/rpc/connection.ConnectionService/StartSession", (req) => {
      const body = toBinary(
        StartSessionResponseSchema,
        create(StartSessionResponseSchema, {
          sessionId: newSessionId,
          livekitRoom: `room-${newSessionId}`,
          livekitUrl: "ws://127.0.0.1:7880",
          livekitServerIdentity: "server-new",
        }),
      );
      req.reply({
        statusCode: 200,
        headers: { "Content-Type": "application/proto" },
        body: toArrayBuffer(body),
      });
    }).as("startSessionMulti");
    interceptTokenForPresence();
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    cy.get(`[data-testid="connect-${SESSION_MULTI_A.sessionId}"]`, { timeout: 8000 }).click();
    cy.wait("@connectSessionMulti");
    cy.get(`[data-testid="start-session-${PROJECT.projectId}"]`, { timeout: 8000 }).click();
    cy.wait("@startSessionMulti");
    cy.get(`[data-testid="connection-attached-terminal-${SESSION_MULTI_A.sessionId}"]`, {
      timeout: 20000,
    }).should("exist");
    cy.get(`[data-testid="connection-attached-terminal-${newSessionId}"]`).should("exist");
  });

  it("ConnectionScreen — inactive ListSessions clears only matching attachment (mocked)", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    let poll = 0;
    interceptAllRpcsWithListSessionsFactory(() => {
      poll += 1;
      if (poll === 1) {
        return [SESSION_MULTI_A, SESSION_MULTI_B];
      }
      return [
        { ...SESSION_MULTI_A, isActive: false, status: "exited", pid: 0 },
        SESSION_MULTI_B,
      ];
    });
    interceptConnectSessionPerSessionId();
    interceptTokenForPresence();
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    cy.get(`[data-testid="connect-${SESSION_MULTI_A.sessionId}"]`, { timeout: 8000 }).click();
    cy.wait("@connectSessionMulti");
    cy.get(`[data-testid="connect-${SESSION_MULTI_B.sessionId}"]`).click();
    cy.wait("@connectSessionMulti");
    cy.get(`[data-testid="connection-attached-terminal-${SESSION_MULTI_A.sessionId}"]`, {
      timeout: 15000,
    }).should("exist");
    cy.get(`[data-testid="connection-attached-terminal-${SESSION_MULTI_B.sessionId}"]`).should(
      "exist",
    );
    cy.wait(5500);
    cy.wait("@listSessions");
    cy.get(`[data-testid="connection-attached-terminal-${SESSION_MULTI_B.sessionId}"]`, {
      timeout: 15000,
    }).should("exist");
    cy.get(`[data-testid="connection-attached-terminal-${SESSION_MULTI_A.sessionId}"]`).should(
      "not.exist",
    );
  });
});
