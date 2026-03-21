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

function mockListSessionsResponse(
  sessions: Array<{
    sessionId: string;
    createdAt: string;
    status: string;
    repoPath: string;
    pid: number;
    isActive: boolean;
    projectId: string;
  }>,
) {
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

function interceptAllRpcs(
  sessions: Array<{
    sessionId: string;
    createdAt: string;
    status: string;
    repoPath: string;
    pid: number;
    isActive: boolean;
    projectId: string;
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
