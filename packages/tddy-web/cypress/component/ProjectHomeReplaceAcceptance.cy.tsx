/**
 * Acceptance: a project whose main is set up offers **Replace main** on its pinned entry, which
 * opens the setup form in replace mode (`#/sessions/project-home/<projectId>/replace`) saying the
 * current main is stopped; starting from it sends `EnsureProjectHomeSession` with a start spec and
 * selects the new main. A recorded main the daemon no longer has (`NOT_FOUND`) opens the setup form.
 *
 * Feature doc: docs/ft/web/session-drawer.md § Pinned main entry.
 * PRD: docs/ft/web/session-drawer.md
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { Code, ConnectError } from "@connectrpc/connect";
import type { ProjectEntry } from "../../src/gen/project_pb";
import { SessionEntrySchema, SessionService } from "../../src/gen/session_pb";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { DEFAULT_TEST_DAEMON, withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aSessionServiceBackend } from "../support/rpc/daemonSessionHostBackend";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { projectHomeSetupPage as setup } from "../support/pages/projectHomeSetupPage";

const ALPHA = "proj-alpha";
const MAIN_SESSION_ID = "home-alpha-0000-0000-0000-000000000000";
const NEW_MAIN_SESSION_ID = "home-alpha-1111-1111-1111-111111111111";

/** A project whose main is set up and listed, on the selected host. */
const ALPHA_WITH_MAIN: Partial<ProjectEntry> = {
  projectId: ALPHA,
  name: "alpha",
  mainBranchRef: "origin/main",
  homeSessionId: MAIN_SESSION_ID,
  daemonInstanceId: DEFAULT_TEST_DAEMON.instanceId,
};

/** The same project registered on another host, which has no main set up for it. */
const ALPHA_ON_ANOTHER_HOST_WITHOUT_MAIN: Partial<ProjectEntry> = {
  ...ALPHA_WITH_MAIN,
  homeSessionId: "",
  daemonInstanceId: "another-host",
};

/**
 * The Sessions screen over a daemon listing `projects` whose `EnsureProjectHomeSession` runs
 * `ensure`.
 */
function theSessionsScreen(
  ensure: () => unknown,
  projects: Partial<ProjectEntry>[] = [ALPHA_WITH_MAIN],
) {
  const backend = aSessionServiceBackend({
    sessions: [
      {
        sessionId: MAIN_SESSION_ID,
        projectId: ALPHA,
        sessionRole: "project-home",
        managedCodebase: true,
        model: "opus",
      },
    ],
    projectsOverride: projects,
  }).onUnary(SessionService.method.ensureProjectHomeSession, ensure);
  mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  return backend;
}

/** A daemon that answers every ensure with a newly started (replacing) main. */
const replacesMain = () => ({
  session: create(SessionEntrySchema, {
    sessionId: NEW_MAIN_SESSION_ID,
    projectId: ALPHA,
    sessionRole: "project-home",
  }),
  outcome: "replaced",
});

/** A daemon that no longer has the recorded main. */
const hasNoMain = () => {
  throw new ConnectError(`project ${ALPHA} has no main session: set one up`, Code.NotFound);
};

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
  window.location.hash = "#/sessions";
});

it("Replace main opens the setup form saying the current main will be stopped", () => {
  // Given
  theSessionsScreen(replacesMain);

  // When
  setup.clickReplaceMain(ALPHA);

  // Then
  setup.assertAtSetupFor(ALPHA, { replacing: true });
  setup.pane(ALPHA).should("contain", "Replace main for alpha");
  setup.replacingNotice(ALPHA).should("exist");
  setup.form(ALPHA).should("exist");
});

it("starting from the replace form ensures main with a start spec and selects the new main", () => {
  // Given
  const backend = theSessionsScreen(replacesMain);
  setup.clickReplaceMain(ALPHA);

  // When
  setup.startMain();

  // Then
  setup.assertEnsuredOnce(backend, { projectId: ALPHA, start: { sessionType: "claude-cli" } });
  setup.assertSessionSelected(NEW_MAIN_SESSION_ID);
});

it("a main the daemon no longer has opens the setup form", () => {
  // Given
  const backend = theSessionsScreen(hasNoMain);

  // When
  setup.clickMain(ALPHA);

  // Then the daemon was asked for main, and its answer sent the operator to set main up
  setup.assertEnsuredOnce(backend, { projectId: ALPHA, start: null });
  setup.assertAtSetupFor(ALPHA);
  setup.pane(ALPHA).should("contain", "Set up main for alpha");
});

it("the pinned entry stands for the selected host's main, not the first-listed host's", () => {
  // When the project is listed first on a host with no main for it, then on the selected host
  theSessionsScreen(replacesMain, [ALPHA_ON_ANOTHER_HOST_WITHOUT_MAIN, ALPHA_WITH_MAIN]);

  // Then the pinned entry stands for the selected host's main
  setup.mainEntry(ALPHA).should("exist");
  setup.replaceAction(ALPHA).should("exist");
});
