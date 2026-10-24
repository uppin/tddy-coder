/**
 * Acceptance: clicking a project's pinned **main** entry when the project has no main opens the
 * setup pane (`#/sessions/project-home/<projectId>`), which is the **New session** form in its main
 * variant — project fixed, host/project/branch/stack/attachments/placement hidden, every session
 * type shown but only Claude CLI selectable, each other type saying why. **Start main session**
 * sends `EnsureProjectHomeSession` carrying the form's start spec and selects the session it
 * returns.
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
import { whyMainCannotRun } from "../../src/components/sessions/projectHomeSessionTypes";
import { ProjectHomeSetupPane } from "../../src/components/sessions/ProjectHomeSetupPane";
import { DEFAULT_TEST_DAEMON, withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aSessionServiceBackend } from "../support/rpc/daemonSessionHostBackend";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { projectHomeSetupPage as setup } from "../support/pages/projectHomeSetupPage";

const BETA = "proj-beta";
const MAIN_SESSION_ID = "home-beta-0000-0000-0000-000000000000";

/** A project the selected daemon lists with no main set up. */
const BETA_WITHOUT_MAIN: Partial<ProjectEntry> = {
  projectId: BETA,
  name: "beta",
  mainBranchRef: "origin/master",
  homeSessionId: "",
  daemonInstanceId: DEFAULT_TEST_DAEMON.instanceId,
};

/**
 * The Sessions screen over a daemon listing `beta` (no main) and one of its sessions.
 * `EnsureProjectHomeSession` answers with the started main, or throws `refusal` when given.
 */
function theSessionsScreen(refusal?: ConnectError) {
  const backend = aSessionServiceBackend({
    sessions: [{ sessionId: "b1", projectId: BETA }],
    projectsOverride: [BETA_WITHOUT_MAIN],
  }).onUnary(SessionService.method.ensureProjectHomeSession, () => {
    if (refusal) throw refusal;
    return {
      session: create(SessionEntrySchema, {
        sessionId: MAIN_SESSION_ID,
        projectId: BETA,
        sessionRole: "project-home",
      }),
      outcome: "started",
    };
  });
  mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  return backend;
}

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
  window.location.hash = "#/sessions";
});

it("clicking main on a project with no main opens the New-session form for that project", () => {
  // Given
  theSessionsScreen();

  // When
  setup.clickMain(BETA);

  // Then
  setup.assertAtSetupFor(BETA);
  setup.pane(BETA).should("contain", "Set up main for beta");
  setup.form(BETA).should("exist");
  setup.submitButton().should("have.text", "Start main session");
});

it("the main form hides the host, project, branch, stack, attachments and placement controls", () => {
  // Given
  theSessionsScreen();

  // When
  setup.clickMain(BETA);

  // Then
  setup.assertMainVariantHidesPlacementAndBranchControls(BETA);
});

it("only Claude CLI can be chosen for main; every other type says why", () => {
  // Given
  theSessionsScreen();

  // When
  setup.clickMain(BETA);

  // Then
  setup.typeButton("claude-cli").should("have.attr", "aria-pressed", "true").and("not.be.disabled");
  setup.typeButton("tool").should("be.disabled");
  setup.typeButton("cursor-cli").should("be.disabled");
  setup.typeButton("opencode-cli").should("be.disabled");
  setup.typeButton("muse-cli").should("be.disabled");
  setup.typeReason("tool").should("have.text", whyMainCannotRun("tool"));
  setup.typeReason("cursor-cli").should("have.text", whyMainCannotRun("cursor-cli"));
  setup.typeReason("opencode-cli").should("have.text", whyMainCannotRun("opencode-cli"));
  setup.typeReason("muse-cli").should("have.text", whyMainCannotRun("muse-cli"));
});

it("Start main session ensures main with the form's model, permission mode and initial prompt, and selects it", () => {
  // Given
  const backend = theSessionsScreen();
  setup.clickMain(BETA);
  setup.chooseModel("sonnet");
  setup.choosePermissionMode("plan");
  setup.typeInitialPrompt("Watch the stack");

  // When
  setup.startMain();

  // Then
  setup.assertEnsuredOnce(backend, {
    projectId: BETA,
    start: { sessionType: "claude-cli", model: "sonnet", permissionMode: "plan", initialPrompt: "Watch the stack" },
  });
  setup.assertSessionSelected(MAIN_SESSION_ID);
  setup.pane(BETA).should("not.exist");
});

it("shows the daemon's reason in the form when setting up main is refused", () => {
  // Given
  theSessionsScreen(new ConnectError("claude is not installed on this host", Code.FailedPrecondition));
  setup.clickMain(BETA);

  // When
  setup.startMain();

  // Then
  setup.error().should("contain", "claude is not installed on this host");
  setup.pane(BETA).should("exist");
});

it("Cancel closes the pane back to the drawer root", () => {
  // Given
  theSessionsScreen();
  setup.clickMain(BETA);

  // When
  setup.cancel();

  // Then
  setup.assertAtDrawerRoot();
  setup.pane(BETA).should("not.exist");
});

it("says so when the host does not list the project, and offers nothing to start", () => {
  // Given
  window.location.hash = "#/sessions/project-home/proj-unlisted";

  // When
  theSessionsScreen();

  // Then
  setup.unknownProject("proj-unlisted").should("contain", "proj-unlisted");
  setup.form("proj-unlisted").should("not.exist");
});

it("says the host is not connected when no wire reaches it, and offers no form", () => {
  // Given a registry row for beta, but no daemon any wire reaches
  const onStart = cy.stub().as("onStart").resolves();

  // When
  cy.mount(
    withSelectedDaemon(
      <ProjectHomeSetupPane
        projectId={BETA}
        project={{ projectId: BETA, name: "beta", mainBranchRef: "origin/master" }}
        replacing={false}
        daemonInstanceId={DEFAULT_TEST_DAEMON.instanceId}
        sessionToken="fake-token"
        onStart={onStart}
        onClose={cy.stub()}
      />,
      [],
    ),
  );

  // Then
  setup.unreachableHost(BETA).should("contain", "Not connected to");
  setup.form(BETA).should("not.exist");
});
