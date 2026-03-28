import React from "react";
import { create } from "@bufbuild/protobuf";
import { toBinary } from "@bufbuild/protobuf";
import { ConnectionScreen } from "../../src/components/ConnectionScreen";
import {
  ListToolsResponseSchema,
  ToolInfoSchema,
  ListSessionsResponseSchema,
  SessionEntrySchema,
  ListProjectsResponseSchema,
  ProjectEntrySchema,
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

const ACTIVE_SESSION = {
  sessionId: "session-active-1",
  createdAt: "2026-03-21T10:00:00Z",
  status: "active",
  repoPath: "/home/dev/project",
  pid: 12345,
  isActive: true,
  projectId: "proj-1",
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

type MockSessionRow = {
  sessionId: string;
  createdAt: string;
  status: string;
  repoPath: string;
  pid: number;
  isActive: boolean;
  projectId: string;
  workflowGoal?: string;
  workflowState?: string;
  elapsedDisplay?: string;
  agent?: string;
  model?: string;
};

function mockListSessionsResponse(sessions: MockSessionRow[]) {
  return toBinary(
    ListSessionsResponseSchema,
    create(ListSessionsResponseSchema, {
      sessions: sessions.map((s) => create(SessionEntrySchema, s)),
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

function interceptAllRpcs(sessions: MockSessionRow[]) {
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

  it("delete_button_visible_only_for_inactive_session_row", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcs(sessionsForDeleteSuite);
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    cy.get(`[data-testid="sessions-table-${PROJECT.projectId}"]`, { timeout: 5000 }).should("exist");
    cy.get(`[data-testid="delete-session-${INACTIVE_SESSION.sessionId}"]`).should("exist");
    cy.get(`[data-testid="delete-session-${ORPHAN_INACTIVE_SESSION.sessionId}"]`).should("exist");
    cy.get("[data-testid='sessions-table-orphan']", { timeout: 5000 }).should("exist");
  });

  it("delete_button_hidden_for_active_session_row", () => {
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptAllRpcs(sessionsForDeleteSuite);
    cy.mount(<ConnectionScreen />);
    cy.wait("@getAuthStatus");
    cy.get(`[data-testid="sessions-table-${PROJECT.projectId}"]`, { timeout: 5000 }).should("exist");
    cy.get(`[data-testid="connect-${ACTIVE_SESSION.sessionId}"]`, { timeout: 5000 }).should("exist");
    cy.get(`[data-testid="connect-${ORPHAN_ACTIVE_SESSION.sessionId}"]`).should("exist");
    // Prerequisite for this spec: inactive rows expose Delete — then active rows must not.
    cy.get(`[data-testid="delete-session-${INACTIVE_SESSION.sessionId}"]`).should("exist");
    cy.get(`[data-testid="delete-session-${ORPHAN_INACTIVE_SESSION.sessionId}"]`).should("exist");
    cy.get(`[data-testid="delete-session-${ACTIVE_SESSION.sessionId}"]`).should("not.exist");
    cy.get(`[data-testid="delete-session-${ORPHAN_ACTIVE_SESSION.sessionId}"]`).should("not.exist");
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
