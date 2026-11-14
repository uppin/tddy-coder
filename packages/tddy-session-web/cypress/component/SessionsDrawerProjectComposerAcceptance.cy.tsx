/**
 * Acceptance: a "+" on each project header in the sessions drawer opens the **orchestrator
 * composer** for that project in the detail area (`#/sessions/project/<projectId>/new`). Sending
 * the text in its box starts an orchestrator for the project — `StartSession` with
 * `session_role: orchestrator` and the text as its first prompt — and selects it as a created
 * session is; a refusal keeps the text and shows the daemon's reason. Every listed project has a
 * header and a "+", even one with no session and no main.
 *
 * The whole Sessions screen over the in-memory ConnectRPC backend (`mountWithRpc`), so the
 * assertions read the typed request the screen sent.
 *
 * Feature: docs/ft/web/session-drawer.md § Orchestrator composer
 * Changeset: docs/dev/changesets/2026-11-14-orchestrator-composer.md
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { Code, ConnectError } from "@connectrpc/connect";
import type { ProjectEntry } from "tddy-client-web/gen/project_pb";
import {
  AgentInfoSchema,
  type AgentInfo,
  CatalogService,
  ListAgentsResponseSchema,
} from "tddy-client-web/gen/catalog_pb";
import { SessionService, StartSessionResponseSchema } from "tddy-client-web/gen/session_pb";
import { SessionsDrawerScreen } from "../../src/SessionsDrawerScreen";
import { DEFAULT_TEST_DAEMON, withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aSessionServiceBackend } from "../support/rpc/daemonSessionHostBackend";
import { mountWithRpc } from "../support/rpc/inMemory";
import { servedAgentWire, withDeclaredSessionTypes } from "../support/servedAgents";
import { projectComposerPage as composer } from "../support/pages/projectComposerPage";

const BETA = "proj-beta";
const GAMMA = "proj-gamma";
const ORCHESTRATOR_ROLE = "orchestrator";
const STARTED_ORCHESTRATOR = "orchestrator-0000-0000-0000-000000000001";

/** A project the selected daemon lists, with no main set up. */
function aListedProject(projectId: string, name: string): Partial<ProjectEntry> {
  return {
    projectId,
    name,
    mainBranchRef: "origin/master",
    homeSessionId: "",
    daemonInstanceId: DEFAULT_TEST_DAEMON.instanceId,
  };
}

/**
 * The Sessions screen over a daemon listing `beta` (one session) and `gamma` (no session, no main),
 * serving Claude — whose spec declares `project_home_main` and two effort levels — and Cursor.
 * `StartSession` answers with the started orchestrator, or throws `refusal` when given.
 */
function theSessionsScreen(refusal?: ConnectError) {
  const backend = aSessionServiceBackend({
    sessions: [{ sessionId: "b1", projectId: BETA }],
    projectsOverride: [aListedProject(BETA, "beta"), aListedProject(GAMMA, "gamma")],
  })
    .onUnary(CatalogService.method.listAgents, () => {
      const agents = [
        create(AgentInfoSchema, {
          id: "cursor",
          label: "Cursor",
          ...(servedAgentWire("cursor") as Omit<Partial<AgentInfo>, "$typeName" | "$unknown">),
        }),
        create(AgentInfoSchema, {
          id: "claude",
          label: "Claude",
          ...(servedAgentWire("claude") as Omit<Partial<AgentInfo>, "$typeName" | "$unknown">),
          effortLevels: ["low", "high"],
        }),
      ];
      return create(ListAgentsResponseSchema, {
        agents,
        sessionTypes: withDeclaredSessionTypes({ agents }).sessionTypes,
      });
    })
    .onUnary(SessionService.method.startSession, () => {
      if (refusal) throw refusal;
      return create(StartSessionResponseSchema, { sessionId: STARTED_ORCHESTRATOR });
    });
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  return backend;
}

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
  window.location.hash = "#/sessions";
});

it("the plus on a project header opens the composer for that project", () => {
  // Given
  theSessionsScreen();

  // When
  composer.openComposerFor(BETA);

  // Then
  composer.assertAtComposerFor(BETA);
  composer.composer(BETA).should("be.visible");
  composer.box().should("be.visible");
});

it("a project with no sessions and no main still shows its header and plus", () => {
  // When
  theSessionsScreen();

  // Then
  composer.groupHeader(GAMMA).should("contain", "gamma");
  composer.plus(GAMMA).should("be.visible");
});

it("sending starts an orchestrator with the text as its initial prompt and selects it", () => {
  // Given
  const backend = theSessionsScreen();
  composer.openComposerFor(BETA);
  composer.write("Plan the next stack");
  composer.chooseEffort("high");

  // When
  composer.send();

  // Then
  composer.assertStartedOnce(backend, {
    sessionRole: ORCHESTRATOR_ROLE,
    sessionType: "claude-cli",
    projectId: BETA,
    initialPrompt: "Plan the next stack",
    effort: "high",
    daemonInstanceId: DEFAULT_TEST_DAEMON.instanceId,
  });
  composer.assertSessionSelected(STARTED_ORCHESTRATOR);
  composer.composer(BETA).should("not.exist");
});

it("a refused start keeps the text and shows the daemon's reason", () => {
  // Given
  theSessionsScreen(new ConnectError("claude is not installed on this host", Code.FailedPrecondition));
  composer.openComposerFor(BETA);
  composer.write("Plan the next stack");

  // When
  composer.send();

  // Then
  composer.error().should("contain", "claude is not installed on this host");
  composer.box().should("have.value", "Plan the next stack");
  composer.assertAtComposerFor(BETA);
});
