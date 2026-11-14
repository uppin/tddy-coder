/**
 * Acceptance: a gear on each project header in the sessions drawer opens the project's **settings
 * pane** in the detail area (`#/sessions/project/<projectId>/settings`), for the drawer's selected
 * host unless the route names one (`?host=`). Main is set up there, and replaced there — **Replace
 * main** moves off the pinned entry into the pane. The old setup routes
 * (`#/sessions/project-home/<projectId>[/replace]`) open the same pane.
 *
 * The whole Sessions screen over the in-memory ConnectRPC backend (`mountWithRpc`), so the
 * assertions read the typed request the screen sent.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-10-project-settings-pane.md
 * Changeset: docs/dev/1-WIP/2026-11-10-project-settings-pane.md
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import type { ProjectEntry } from "tddy-client-web/gen/project_pb";
import {
  EnsureProjectHomeSessionResponseSchema,
  SessionEntrySchema,
  SessionService,
} from "tddy-client-web/gen/session_pb";
import { SessionsDrawerScreen } from "../../src/SessionsDrawerScreen";
import { DEFAULT_TEST_DAEMON, withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aSessionServiceBackend } from "../support/rpc/daemonSessionHostBackend";
import { mountWithRpc } from "../support/rpc/inMemory";
import { projectSettingsPage as settings } from "../support/pages/projectSettingsPage";
import { projectHomeSetupPage as main } from "../support/pages/projectHomeSetupPage";

const ALPHA = "proj-alpha";
const BETA = "proj-beta";
const ANOTHER_HOST = "another-host";
const ALPHA_MAIN = "home-alpha-0000-0000-0000-000000000000";
const NEW_MAIN = "home-new-1111-1111-1111-111111111111";
/** Beta's main on the other host — a session the selected host does not list. */
const BETA_MAIN_ELSEWHERE = "home-beta-2222-2222-2222-222222222222";

/** `projectId` as `host` lists it, with `homeSessionId` its main there (empty: none). */
function aProjectRow(
  projectId: string,
  name: string,
  { host = DEFAULT_TEST_DAEMON.instanceId, homeSessionId = "" }: { host?: string; homeSessionId?: string } = {},
): Partial<ProjectEntry> {
  return { projectId, name, mainBranchRef: "origin/main", homeSessionId, daemonInstanceId: host };
}

/** On the selected host: alpha with its main set up, beta with none. */
const ALPHA_WITH_MAIN = aProjectRow(ALPHA, "alpha", { homeSessionId: ALPHA_MAIN });
const BETA_WITHOUT_MAIN = aProjectRow(BETA, "beta");

/** A daemon that answers every ensure with a newly started main. */
const startsMain = () =>
  create(EnsureProjectHomeSessionResponseSchema, {
    session: create(SessionEntrySchema, { sessionId: NEW_MAIN, sessionRole: "project-home" }),
    outcome: "started",
  });

/**
 * The Sessions screen over a daemon listing `projects` and alpha's main session, whose
 * `EnsureProjectHomeSession` starts a new main.
 */
function theSessionsScreen(projects: Partial<ProjectEntry>[] = [ALPHA_WITH_MAIN, BETA_WITHOUT_MAIN]) {
  const backend = aSessionServiceBackend({
    sessions: [
      { sessionId: ALPHA_MAIN, projectId: ALPHA, sessionRole: "project-home", managedCodebase: true, model: "opus" },
      { sessionId: "b1", projectId: BETA },
    ],
    projectsOverride: projects,
  }).onUnary(SessionService.method.ensureProjectHomeSession, startsMain);
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  return backend;
}

/** The Sessions screen, opened at `hash`. */
function theSessionsScreenAt(hash: string, projects?: Partial<ProjectEntry>[]) {
  window.location.hash = hash;
  return theSessionsScreen(projects);
}

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
  window.location.hash = "#/sessions";
});

it("the gear opens the settings pane for that project on the selected host", () => {
  // Given beta is listed first on another host whose main is set up, then on the selected host,
  // which has none
  theSessionsScreen([aProjectRow(BETA, "beta", { host: ANOTHER_HOST, homeSessionId: BETA_MAIN_ELSEWHERE }), BETA_WITHOUT_MAIN]);

  // When
  settings.openSettingsFor(BETA);

  // Then the pane is beta's, and its main is the selected host's: not set up
  settings.assertAtSettingsFor(BETA);
  settings.pane(BETA).should("be.visible");
  settings.assertMainIsNotSetUp(BETA);
  settings.defaultsSection(BETA).should("be.visible");
});

it("main is set up from the settings pane", () => {
  // Given
  const backend = theSessionsScreen();
  settings.openSettingsFor(BETA);

  // When
  main.startMain();

  // Then
  main.assertEnsuredOnce(backend, { projectId: BETA, start: { sessionType: "claude-cli" } });
  main.assertSessionSelected(NEW_MAIN);
});

it("main is replaced from the settings pane, not from the pinned entry", () => {
  // Given
  const backend = theSessionsScreen();
  main.mainEntry(ALPHA).should("be.visible");
  settings.assertPinnedEntryOffersNoReplace(ALPHA);
  settings.openSettingsFor(ALPHA);

  // When
  settings.clickReplaceMain(ALPHA);
  settings.replacingNotice(ALPHA).should("be.visible");
  main.startMain();

  // Then
  main.assertEnsuredOnce(backend, { projectId: ALPHA, start: { sessionType: "claude-cli" } });
  main.assertSessionSelected(NEW_MAIN);
});

it("the old project-home route opens the settings pane", () => {
  // When
  theSessionsScreenAt(`#/sessions/project-home/${BETA}`);

  // Then
  settings.pane(BETA).should("be.visible");
  settings.assertMainIsNotSetUp(BETA);
  settings.defaultsSection(BETA).should("be.visible");
});

it("the old replace route opens the settings pane replacing main", () => {
  // When
  theSessionsScreenAt(`#/sessions/project-home/${ALPHA}/replace`);

  // Then
  settings.pane(ALPHA).should("be.visible");
  settings.replacingNotice(ALPHA).should("be.visible");
});

it("a settings route naming another host acts on that host's main", () => {
  // When alpha's main is set up on the selected host but not on the host the route names
  theSessionsScreenAt(`#/sessions/project/${ALPHA}/settings?host=${ANOTHER_HOST}`, [
    ALPHA_WITH_MAIN,
    aProjectRow(ALPHA, "alpha", { host: ANOTHER_HOST }),
  ]);

  // Then the pane shows the named host's main: not set up
  settings.pane(ALPHA).should("be.visible");
  settings.assertMainIsNotSetUp(ALPHA);
});
