/**
 * Acceptance tests: creating a session on a branch another session already owns prompts the operator
 * instead of silently creating a `<branch>-1` suffixed branch.
 *
 * The daemon refuses the creation and reports `StartSessionResponse.branch_conflict` (a populated
 * response field, not an RPC error — see the PRD for why). The form turns that into a three-choice
 * dialog: switch to the owning session, add a second agent on the same branch, or name a different
 * branch.
 *
 * PRD: docs/ft/daemon/session-branch-conflict.md
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { ConnectionService, type SessionEntry } from "../../src/gen/connection_pb";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { aConnectionServiceBackend } from "../support/rpc/connectionServiceBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { createSessionPage } from "../support/pages/createSessionPage";
import { branchConflictDialogPage } from "../support/pages/branchConflictDialogPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const PROJECT_ID = "proj-branch-conflict";
const AGENT_ID = "claude";
/** The branch the operator asks for, already owned by OWNER_SESSION_ID. */
const OWNED_BRANCH = "feat/auth";
/** The name the daemon suggests instead — what the legacy suffixing path would have created. */
const SUGGESTED_BRANCH = "feat/auth-1";
const FREE_BRANCH = "feat/auth-rewrite";

const OWNER_SESSION_ID = "owner-session-0000-0000-0000-000000000001";
const CREATED_SESSION_ID = "created-session-0000-0000-0000-000000000002";

const OWNER_SESSION: Partial<SessionEntry> = {
  sessionId: OWNER_SESSION_ID,
  createdAt: "2026-07-30T09:00:00Z",
  status: "active",
  repoPath: "/home/dev/branch-conflict-project/feat-auth",
  pid: 4242,
  isActive: true,
  projectId: PROJECT_ID,
  branch: OWNED_BRANCH,
  orchestratorSessionId: "",
};

/** The refusal the daemon returns: nothing was created, and this session owns the branch. */
function aRefusalFor(branch: string) {
  return {
    sessionId: "",
    livekitRoom: "",
    livekitUrl: "",
    livekitServerIdentity: "",
    branchConflict: {
      branch,
      owner: {
        exists: true,
        sessionId: OWNER_SESSION_ID,
        isActive: true,
        status: "active",
      },
      suggestedBranchName: `${branch}-1`,
    },
  };
}

/** A successful creation. */
function aCreation() {
  return {
    sessionId: CREATED_SESSION_ID,
    livekitRoom: `room-${CREATED_SESSION_ID}`,
    livekitUrl: "ws://127.0.0.1:7880",
    livekitServerIdentity: "server",
  };
}

/**
 * A backend that refuses the first `refusals` StartSession calls with a branch conflict naming the
 * branch each call requested, then creates the session. The owning session is in the drawer so the
 * "switch" choice has somewhere to go.
 */
function aBackendRefusing(refusals: number) {
  let calls = 0;
  return aConnectionServiceBackend({
    sessions: [OWNER_SESSION],
    projectsOverride: [{ projectId: PROJECT_ID, name: "Branch Conflict Project" }],
    agents: [{ id: AGENT_ID, label: "Claude (opus)" }],
    connectSession: { livekitRoom: `room-${OWNER_SESSION_ID}` },
  }).onUnary(ConnectionService.method.startSession, (req) => {
    calls += 1;
    return calls <= refusals ? aRefusalFor(req.newBranchName) : aCreation();
  });
}

/** Fill the creation form for a tool session on `branch` and submit it. */
function createSessionOnBranch(branch: string) {
  sessionsDrawerPage.newSessionBtn().click();
  createSessionPage.selectProject(PROJECT_ID);
  createSessionPage.selectAgent(AGENT_ID);
  createSessionPage.typeNewBranchName(branch);
  sessionsDrawerPage.createSessionSubmitBtn().should("not.be.disabled").click();
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  // `SessionsDrawerScreen` reads the route on mount, and a test that navigates leaves the hash
  // behind for the next one — reset it so every test starts on the session list.
  window.location.hash = "";
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

it("asks the daemon to reject an owned branch instead of suffixing it", () => {
  // Given
  const backend = aBackendRefusing(0);

  // When
  mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  createSessionOnBranch(FREE_BRANCH);

  // Then — without this field the daemon silently creates `<branch>-1`.
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ConnectionService.method.startSession);
    expect(calls).to.have.length(1);
    expect(calls[0].onBranchConflict).to.equal("reject");
  });
});

it("opens the branch-conflict dialog and keeps the creation form when the branch is owned", () => {
  // Given
  const backend = aBackendRefusing(1);

  // When
  mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  createSessionOnBranch(OWNED_BRANCH);

  // Then — the refusal is recognised as a prompt, not as a created session.
  branchConflictDialogPage.dialog().should("be.visible");
  sessionsDrawerPage.createSessionPane().should("be.visible");
});

it("names the owning session and that it is active", () => {
  // Given
  const backend = aBackendRefusing(1);

  // When
  mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  createSessionOnBranch(OWNED_BRANCH);

  // Then — the operator can tell what they would be switching to.
  branchConflictDialogPage.owner().should("contain.text", OWNER_SESSION_ID);
  branchConflictDialogPage.owner().should("contain.text", "active");
});

it("attaches to the owning session when Switch is chosen, without creating a session", () => {
  // Given
  const backend = aBackendRefusing(1);

  // When
  mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  createSessionOnBranch(OWNED_BRANCH);
  branchConflictDialogPage.chooseSwitch();

  // Then — the owning session is attached and no second creation was attempted.
  cy.wrap(backend).should((b) => {
    const connects = b.callsTo(ConnectionService.method.connectSession);
    expect(connects.map((c) => c.sessionId)).to.include(OWNER_SESSION_ID);
    expect(b.callsTo(ConnectionService.method.startSession)).to.have.length(1);
  });
  branchConflictDialogPage.dialog().should("not.exist");
});

it("starts a second agent on the owned branch when Add another agent is chosen", () => {
  // Given
  const backend = aBackendRefusing(1);

  // When
  mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  createSessionOnBranch(OWNED_BRANCH);
  branchConflictDialogPage.chooseAddAgent();

  // Then — the re-submission joins the existing branch, which reuses the owner's worktree.
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ConnectionService.method.startSession);
    expect(calls).to.have.length(2);
    expect(calls[1].branchWorktreeIntent).to.equal("work_on_selected_branch");
    expect(calls[1].selectedBranchToWorkOn).to.equal(OWNED_BRANCH);
    expect(calls[1].newBranchName).to.equal("");
  });
});

it("pre-fills the rename field with the branch name the daemon suggests", () => {
  // Given
  const backend = aBackendRefusing(1);

  // When
  mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  createSessionOnBranch(OWNED_BRANCH);

  // Then — the suffix the legacy path would have used is offered explicitly, not applied silently.
  branchConflictDialogPage.renameInput().should("have.value", SUGGESTED_BRANCH);
});

it("creates the session under the typed branch name when the rename is submitted", () => {
  // Given
  const backend = aBackendRefusing(1);

  // When
  mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  createSessionOnBranch(OWNED_BRANCH);
  branchConflictDialogPage.renameTo(FREE_BRANCH);

  // Then
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ConnectionService.method.startSession);
    expect(calls).to.have.length(2);
    expect(calls[1].branchWorktreeIntent).to.equal("new_branch_from_base");
    expect(calls[1].newBranchName).to.equal(FREE_BRANCH);
  });
  branchConflictDialogPage.dialog().should("not.exist");
});

it("re-opens the dialog when the typed branch name is also owned", () => {
  // Given — every creation is refused, so the renamed branch collides too.
  const backend = aBackendRefusing(2);

  // When
  mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  createSessionOnBranch(OWNED_BRANCH);
  branchConflictDialogPage.renameTo(FREE_BRANCH);

  // Then — the prompt is re-entrant and now suggests a suffix of the newly typed name.
  branchConflictDialogPage.dialog().should("be.visible");
  branchConflictDialogPage.renameInput().should("have.value", `${FREE_BRANCH}-1`);
});

it("returns to the filled creation form when the dialog is cancelled", () => {
  // Given
  const backend = aBackendRefusing(1);

  // When
  mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  createSessionOnBranch(OWNED_BRANCH);
  branchConflictDialogPage.cancel();

  // Then — nothing more was created and the form still holds what the operator typed.
  branchConflictDialogPage.dialog().should("not.exist");
  sessionsDrawerPage.createSessionPane().should("be.visible");
  cy.wrap(backend).should((b) => {
    expect(b.callsTo(ConnectionService.method.startSession)).to.have.length(1);
  });
});
