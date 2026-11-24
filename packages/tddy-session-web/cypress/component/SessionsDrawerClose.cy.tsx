/**
 * Closing a session from its drawer row: an active row's ×, an ended row's trash bin while it still
 * holds a worktree, the close dialog that lists what the close would lose before anything is
 * terminated or removed, the per-row spinner while a close runs, concurrent closes, the children
 * choice, a daemon that asks again, and the drawer notice for a close whose steps failed.
 *
 * PRD: docs/ft/web/session-drawer.md § Closing a session
 * Contract: packages/tddy-service/proto/session.proto (PreviewCloseSession, CloseSession,
 * SessionEntry.holds_worktree)
 */

import React from "react";
import type { SessionEntry } from "tddy-client-web/gen/session_pb";
import type { ProjectEntry } from "tddy-client-web/gen/project_pb";
import {
  CloseSessionStepKind,
  CloseSessionStepState,
} from "tddy-client-web/gen/session_pb";
import { SessionsDrawerScreen } from "../../src/SessionsDrawerScreen";
import { DEFAULT_TEST_DAEMON, withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import {
  aSessionServiceBackend,
  type SessionServiceBackend,
  type SessionServiceScenario,
} from "../support/rpc/daemonSessionHostBackend";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import {
  aClosePreviewWithNothingAtRisk,
  aCloseThatAsksAgain,
  aCloseThatFails,
  aCloseThatRunsUntilReleased,
  aConversationWorktree,
  aSessionWorktree,
  type ClosePreviewInit,
} from "../support/rpc/closeSessionBackend";
import { closeSessionPage as close } from "../support/pages/closeSessionPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const PROJECT_ID = "proj-tddy-coder";

const TDDY_CODER: Partial<ProjectEntry> = {
  projectId: PROJECT_ID,
  name: "tddy-coder",
  gitUrl: "git@github.com:tddy/tddy-coder.git",
  mainRepoPath: "/home/dev/src/tddy-coder",
  mainBranchRef: "origin/master",
  daemonInstanceId: DEFAULT_TEST_DAEMON.instanceId,
  homeSessionId: "",
};

function aRunningSession(sessionId: string, workflowGoal: string, overrides: Partial<SessionEntry> = {}): Partial<SessionEntry> {
  return {
    sessionId,
    projectId: PROJECT_ID,
    workflowGoal,
    createdAt: "2026-11-24T09:15:00Z",
    status: "active",
    isActive: true,
    pid: 48213,
    repoPath: `/home/dev/.tddy/worktrees/tddy-coder/${sessionId.slice(0, 8)}`,
    holdsWorktree: true,
    ...overrides,
  };
}

function anEndedSession(sessionId: string, workflowGoal: string, overrides: Partial<SessionEntry> = {}): Partial<SessionEntry> {
  return aRunningSession(sessionId, workflowGoal, {
    status: "exited",
    isActive: false,
    pid: 0,
    createdAt: "2026-11-23T16:40:00Z",
    updatedAt: "2026-11-23T18:05:00Z",
    ...overrides,
  });
}

const LIVEKIT_FIX = aRunningSession("4b1d9e2a-7c3f-4a8e-9d61-2f0c5e8b7a10", "Fix the flaky LiveKit reconnect test");
const DRAWER_CLOSE = aRunningSession("9e6a2c41-3d8b-4f17-a5c2-8b0e1d7f4c22", "Close sessions from the drawer");
const OSC52_COPY = aRunningSession("1a5c7e9b-4d2f-4b6a-8c3e-5f7a9b1d2e55", "Copy terminal selection over OSC 52");
const ENDED_WITH_WORKTREE = anEndedSession("c2f8a5d3-6e1b-4c9a-8f73-0d4b2e9a1c33", "Render dormant tmux tabs", {
  holdsWorktree: true,
});
const ENDED_WITHOUT_WORKTREE = anEndedSession("e7b3d1f9-2a6c-4e58-b0d4-6c1f8a3e5d44", "Rename sessions inline", {
  holdsWorktree: false,
});

/** An orchestrator and the sessions it started. */
const STACK_ORCHESTRATOR = aRunningSession("5d2b8f4e-1c7a-4e3b-9a6d-3e8c1f5b7a66", "Orchestrate the #session-close stack", {
  recipe: "orchestrate-pr-stack",
});
const CHILD_RUNNING = aRunningSession("6f3c9a5b-2d8e-4f4c-8b7a-4f9d2a6c8b77", "Node 1: the close-session contract", {
  orchestratorSessionId: STACK_ORCHESTRATOR.sessionId,
});
const CHILD_ENDED_WITH_WORKTREE = anEndedSession("7a4d0b6c-3e9f-4a5d-9c8b-5a0e3b7d9c88", "Node 2: the drawer's close dialog", {
  orchestratorSessionId: STACK_ORCHESTRATOR.sessionId,
  holdsWorktree: true,
});
const CHILD_ENDED_WITHOUT_WORKTREE = anEndedSession("8b5e1c7d-4f0a-4b6e-8d9c-6b1f4c8e0d99", "Node 0: plan the stack", {
  orchestratorSessionId: STACK_ORCHESTRATOR.sessionId,
  holdsWorktree: false,
});

/** The project's main session, pinned at the top of its group. */
const PROJECT_MAIN = aRunningSession("0c9e7a5b-3f1d-4e2c-a8b6-7d5f3e1c9a00", "", {
  sessionRole: "project-home",
  managedCodebase: true,
  model: "opus",
});

const DIRTY_FINGERPRINT = "sha256:5c1f0e9a7d3b2c48e6f1a0b9d8c7e6f5a4b3c2d1e0f9a8b7c6d5e4f3a2b1c0d9";

/** Two worktrees with work on them: the session's own on its ssh target, and a subagent clone. */
const DIRTY_PREVIEW: ClosePreviewInit = {
  worktrees: [
    aSessionWorktree({
      path: "/home/dev/.tddy/worktrees/tddy-coder/9e6a2c41",
      host: "dev-box-3",
      branch: "feature/session-close-and-remove",
      dirtyFiles: [
        { path: "packages/tddy-session-web/src/SessionDrawerItem.tsx", change: "modified" },
        { path: "packages/tddy-session-web/src/CloseSessionDialog.tsx", change: "untracked" },
      ],
      unpushedCommits: [
        { sha: "3f9c2e1a7b4d", subject: "feat(session-web): close a session from its drawer row" },
        { sha: "8d41b06c2e5f", subject: "test(session-web): the close dialog lists every worktree" },
      ],
      unpushedCommitCount: 2,
    }),
    aConversationWorktree("conv-7a1e93", {
      host: "",
      branch: "subagent/conv-7a1e93",
      dirtyFiles: [{ path: "packages/tddy-worktree-service/src/worktree_loss.rs", change: "deleted" }],
    }),
  ],
  tabs: [
    { tabId: "main", label: "claude (opus)" },
    { tabId: "conversation:conv-7a1e93", label: "codex" },
  ],
  fingerprint: DIRTY_FINGERPRINT,
  needsConfirmation: true,
};

const UNREADABLE_REASON = "target dev-box-3 unreachable: ssh: connect to host dev-box-3 port 22: Connection refused";

/** The session's worktree is on a target that cannot be reached. */
const UNREADABLE_PREVIEW: ClosePreviewInit = {
  worktrees: [
    aSessionWorktree({
      path: "/home/dev/.tddy/worktrees/tddy-coder/9e6a2c41",
      host: "dev-box-3",
      branch: "feature/session-close-and-remove",
      unavailableReason: UNREADABLE_REASON,
    }),
  ],
  fingerprint: "sha256:0a7e4c2f9b1d8e6c5a3f2b1e0d9c8b7a6f5e4d3c2b1a0f9e8d7c6b5a4f3e2d1c",
  needsConfirmation: true,
};

/** The same worktree once the target answers again: readable, with one file at risk. */
const READABLE_AFTER_RETRY: ClosePreviewInit = {
  worktrees: [
    aSessionWorktree({
      path: "/home/dev/.tddy/worktrees/tddy-coder/9e6a2c41",
      host: "dev-box-3",
      branch: "feature/session-close-and-remove",
      dirtyFiles: [{ path: "packages/tddy-session-web/src/useCloseSession.ts", change: "modified" }],
    }),
  ],
  fingerprint: "sha256:d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5",
  needsConfirmation: true,
};

/** What the daemon finds at its check after an agent wrote one more file in the meantime. */
const GREW_FINGERPRINT = "sha256:7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c";
const PREVIEW_THAT_GREW: ClosePreviewInit = {
  ...DIRTY_PREVIEW,
  worktrees: [
    aSessionWorktree({
      path: "/home/dev/.tddy/worktrees/tddy-coder/9e6a2c41",
      host: "dev-box-3",
      branch: "feature/session-close-and-remove",
      dirtyFiles: [
        { path: "packages/tddy-session-web/src/SessionDrawerItem.tsx", change: "modified" },
        { path: "packages/tddy-session-web/src/CloseSessionDialog.tsx", change: "untracked" },
        { path: "packages/tddy-session-web/src/closeSessionNotices.ts", change: "untracked" },
      ],
    }),
  ],
  fingerprint: GREW_FINGERPRINT,
};

const ORCHESTRATOR_FINGERPRINT = "sha256:1e2d3c4b5a69788796a5b4c3d2e1f0a9b8c7d6e5f4a3b2c1d0e9f8a7b6c5d4e3";
const CHILD_RUNNING_FINGERPRINT = "sha256:2f3e4d5c6b7a89908a7b6c5d4e3f2a1b0c9d8e7f6a5b4c3d2e1f0a9b8c7d6e5f";
const CHILD_ENDED_FINGERPRINT = "sha256:3a4b5c6d7e8f90a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5";

const CHILD_RUNNING_PREVIEW: ClosePreviewInit = {
  worktrees: [
    aSessionWorktree({
      path: "/home/dev/.tddy/worktrees/tddy-coder/6f3c9a5b",
      branch: "feature/close-session-contract",
      dirtyFiles: [{ path: "packages/tddy-service/proto/session.proto", change: "modified" }],
    }),
  ],
  fingerprint: CHILD_RUNNING_FINGERPRINT,
  needsConfirmation: true,
};

const CHILD_ENDED_PREVIEW: ClosePreviewInit = {
  worktrees: [
    aSessionWorktree({
      path: "/home/dev/.tddy/worktrees/tddy-coder/7a4d0b6c",
      branch: "feature/close-session-dialog",
      unpushedCommits: [{ sha: "a17c3e9d0b42", subject: "feat(session-web): the close dialog" }],
      unpushedCommitCount: 1,
    }),
  ],
  fingerprint: CHILD_ENDED_FINGERPRINT,
  needsConfirmation: true,
};

/** The orchestrator's own worktree has nothing at risk; only its children make the close ask. */
const STACK_SCENARIO: SessionServiceScenario = {
  sessions: [STACK_ORCHESTRATOR, CHILD_RUNNING, CHILD_ENDED_WITH_WORKTREE, CHILD_ENDED_WITHOUT_WORKTREE],
  closePreviews: {
    [STACK_ORCHESTRATOR.sessionId!]: aClosePreviewWithNothingAtRisk({ fingerprint: ORCHESTRATOR_FINGERPRINT }),
    [CHILD_RUNNING.sessionId!]: CHILD_RUNNING_PREVIEW,
    [CHILD_ENDED_WITH_WORKTREE.sessionId!]: CHILD_ENDED_PREVIEW,
  },
};

const id = (session: Partial<SessionEntry>) => session.sessionId!;

/** The Sessions screen over a daemon serving `scenario` for the tddy-coder project. */
function theDrawerOver(scenario: SessionServiceScenario): SessionServiceBackend {
  const backend = aSessionServiceBackend({ projectsOverride: [TDDY_CODER], ...scenario });
  mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen onNavigate={cy.stub()} />), backend);
  return backend;
}

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

describe("Sessions drawer — closing a session", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  describe("row controls", () => {
    it("offers the × on an active row and not on an ended one", () => {
      // Given
      theDrawerOver({ sessions: [LIVEKIT_FIX, ENDED_WITH_WORKTREE] });

      // When
      close.revealEndedRow(id(ENDED_WITH_WORKTREE));

      // Then
      close.expectCloseControl(id(LIVEKIT_FIX));
      close.expectNoCloseControl(id(ENDED_WITH_WORKTREE));
    });

    it("offers the trash bin on an ended row that still holds a worktree, and none on one that does not", () => {
      // Given
      theDrawerOver({ sessions: [ENDED_WITH_WORKTREE, ENDED_WITHOUT_WORKTREE] });

      // When
      close.revealEndedRow(id(ENDED_WITH_WORKTREE));

      // Then
      close.expectRemoveWorktreeControl(id(ENDED_WITH_WORKTREE));
      close.expectNoRemoveWorktreeControl(id(ENDED_WITHOUT_WORKTREE));
    });

    it("offers no trash bin on an active row, which has its ×", () => {
      // Given
      theDrawerOver({ sessions: [LIVEKIT_FIX] });

      // Then
      close.expectCloseControl(id(LIVEKIT_FIX));
      close.expectNoRemoveWorktreeControl(id(LIVEKIT_FIX));
    });

    it("offers no × on the project's pinned main", () => {
      // Given — main is pinned at the top of tddy-coder's group, beside an ordinary active row
      theDrawerOver({
        projectsOverride: [{ ...TDDY_CODER, homeSessionId: id(PROJECT_MAIN) }],
        sessions: [PROJECT_MAIN, LIVEKIT_FIX],
      });

      // Then
      close.expectCloseControl(id(LIVEKIT_FIX));
      close.expectPinnedMainHasNoCloseControls(PROJECT_ID, id(PROJECT_MAIN));
    });

    it("offers no trash bin on the project's pinned main when it has ended holding a worktree", () => {
      // Given
      theDrawerOver({
        projectsOverride: [{ ...TDDY_CODER, homeSessionId: id(PROJECT_MAIN) }],
        sessions: [
          { ...PROJECT_MAIN, isActive: false, status: "exited", pid: 0, holdsWorktree: true },
          ENDED_WITH_WORKTREE,
        ],
      });

      // When
      close.revealEndedRow(id(ENDED_WITH_WORKTREE));

      // Then
      close.expectRemoveWorktreeControl(id(ENDED_WITH_WORKTREE));
      close.expectPinnedMainHasNoCloseControls(PROJECT_ID, id(PROJECT_MAIN));
    });
  });

  describe("a session with nothing to lose", () => {
    it("closes at once, without a dialog, sending one unconfirmed close", () => {
      // Given
      const backend = theDrawerOver({ sessions: [LIVEKIT_FIX] });

      // When
      close.clickClose(id(LIVEKIT_FIX));

      // Then
      close.expectClosesRequested(backend, [{ sessionId: id(LIVEKIT_FIX), confirmed: false }]);
      close.expectNoDialog();
    });

    it("asks the session's host what the close would lose before closing", () => {
      // Given
      const backend = theDrawerOver({ sessions: [LIVEKIT_FIX] });

      // When
      close.clickClose(id(LIVEKIT_FIX));

      // Then
      close.expectPreviewsAskedFor(backend, [id(LIVEKIT_FIX)]);
    });

    it("closes an ended session's worktree from its trash bin the same way", () => {
      // Given
      const backend = theDrawerOver({ sessions: [ENDED_WITH_WORKTREE] });

      // When
      close.clickRemoveWorktree(id(ENDED_WITH_WORKTREE));

      // Then
      close.expectClosesRequested(backend, [{ sessionId: id(ENDED_WITH_WORKTREE), confirmed: false }]);
      close.expectNoDialog();
    });
  });

  describe("the close dialog", () => {
    const DIRTY_SCENARIO: SessionServiceScenario = {
      sessions: [DRAWER_CLOSE],
      closePreviews: { [id(DRAWER_CLOSE)]: DIRTY_PREVIEW },
    };

    it("opens before any close is sent when the session would lose work", () => {
      // Given
      const backend = theDrawerOver(DIRTY_SCENARIO);

      // When
      close.clickClose(id(DRAWER_CLOSE));

      // Then
      close.expectDialogOpen();
      close.expectNoCloseRequest(backend);
    });

    it("has one section per worktree the close would remove", () => {
      // Given
      theDrawerOver(DIRTY_SCENARIO);

      // When
      close.clickClose(id(DRAWER_CLOSE));

      // Then
      close.expectWorktreeSectionCount(2);
    });

    it("names the host and the branch of each worktree", () => {
      // Given
      theDrawerOver(DIRTY_SCENARIO);

      // When
      close.clickClose(id(DRAWER_CLOSE));

      // Then
      close.expectWorktreeSectionShows(0, "dev-box-3", "feature/session-close-and-remove");
      close.expectWorktreeSectionShows(1, "subagent/conv-7a1e93");
    });

    it("names the conversation a subagent's worktree belongs to", () => {
      // Given
      theDrawerOver(DIRTY_SCENARIO);

      // When
      close.clickClose(id(DRAWER_CLOSE));

      // Then
      close.expectWorktreeSectionShows(1, "conv-7a1e93");
    });

    it("lists each uncommitted file of a worktree with its change", () => {
      // Given
      theDrawerOver(DIRTY_SCENARIO);

      // When
      close.clickClose(id(DRAWER_CLOSE));

      // Then
      close.expectWorktreeSectionShows(
        0,
        "packages/tddy-session-web/src/SessionDrawerItem.tsx",
        "modified",
        "packages/tddy-session-web/src/CloseSessionDialog.tsx",
        "untracked",
      );
      close.expectWorktreeSectionShows(1, "packages/tddy-worktree-service/src/worktree_loss.rs", "deleted");
    });

    it("lists the subject of each commit no remote holds", () => {
      // Given
      theDrawerOver(DIRTY_SCENARIO);

      // When
      close.clickClose(id(DRAWER_CLOSE));

      // Then
      close.expectWorktreeSectionShows(
        0,
        "feat(session-web): close a session from its drawer row",
        "test(session-web): the close dialog lists every worktree",
      );
    });

    it("closes on Cancel and sends no close", () => {
      // Given
      const backend = theDrawerOver(DIRTY_SCENARIO);
      close.clickClose(id(DRAWER_CLOSE));

      // When
      close.cancel();

      // Then
      close.expectNoDialog();
      close.expectNoCloseRequest(backend);
    });

    it("sends one confirmed close carrying the preview's fingerprint on Confirm", () => {
      // Given
      const backend = theDrawerOver(DIRTY_SCENARIO);
      close.clickClose(id(DRAWER_CLOSE));

      // When
      close.confirm();

      // Then
      close.expectCloseRequests(backend, [
        { sessionId: id(DRAWER_CLOSE), confirmed: true, confirmedFingerprint: DIRTY_FINGERPRINT },
      ]);
    });
  });

  describe("a worktree that could not be read", () => {
    const UNREADABLE_SCENARIO: SessionServiceScenario = {
      sessions: [DRAWER_CLOSE],
      closePreviews: { [id(DRAWER_CLOSE)]: [UNREADABLE_PREVIEW, READABLE_AFTER_RETRY] },
    };

    it("shows why it could not be read", () => {
      // Given
      theDrawerOver(UNREADABLE_SCENARIO);

      // When
      close.clickClose(id(DRAWER_CLOSE));

      // Then
      close.expectWorktreeSectionShows(0, UNREADABLE_REASON);
    });

    it("keeps Confirm disabled", () => {
      // Given
      theDrawerOver(UNREADABLE_SCENARIO);

      // When
      close.clickClose(id(DRAWER_CLOSE));

      // Then
      close.expectConfirmDisabled();
    });

    it("asks the host for the preview again on Retry", () => {
      // Given
      const backend = theDrawerOver(UNREADABLE_SCENARIO);
      close.clickClose(id(DRAWER_CLOSE));

      // When
      close.retry();

      // Then
      close.expectPreviewsAskedFor(backend, [id(DRAWER_CLOSE), id(DRAWER_CLOSE)]);
    });

    it("enables Confirm once the retried preview is readable", () => {
      // Given
      theDrawerOver(UNREADABLE_SCENARIO);
      close.clickClose(id(DRAWER_CLOSE));

      // When
      close.retry();

      // Then
      close.expectWorktreeSectionShows(0, "packages/tddy-session-web/src/useCloseSession.ts");
      close.expectConfirmEnabled();
    });
  });

  describe("a session that started others", () => {
    it("opens the dialog with the children choice even when the session itself is clean", () => {
      // Given
      const backend = theDrawerOver(STACK_SCENARIO);

      // When
      close.clickClose(id(STACK_ORCHESTRATOR));

      // Then
      close.expectDialogOpen();
      close.expectChildrenChoiceOffered();
      close.expectNoCloseRequest(backend);
    });

    it("leaves the children running by default, closing only the session itself", () => {
      // Given
      const backend = theDrawerOver(STACK_SCENARIO);
      close.clickClose(id(STACK_ORCHESTRATOR));

      // When
      close.confirm();

      // Then
      close.expectCloseRequests(backend, [
        { sessionId: id(STACK_ORCHESTRATOR), confirmed: true, confirmedFingerprint: ORCHESTRATOR_FINGERPRINT },
      ]);
    });

    it("adds each child's own losses to the dialog when Close them too is chosen", () => {
      // Given
      theDrawerOver(STACK_SCENARIO);
      close.clickClose(id(STACK_ORCHESTRATOR));

      // When
      close.chooseCloseChildrenToo();

      // Then
      close.expectDialogShows(
        "packages/tddy-service/proto/session.proto",
        "feat(session-web): the close dialog",
      );
    });

    it("closes the session, then each running or worktree-holding child with its own fingerprint", () => {
      // Given
      const backend = theDrawerOver(STACK_SCENARIO);
      close.clickClose(id(STACK_ORCHESTRATOR));
      close.chooseCloseChildrenToo();
      close.expectDialogShows("packages/tddy-service/proto/session.proto");

      // When
      close.confirm();

      // Then — the ended child that holds no worktree is not closed
      close.expectCloseRequestsParentThen(
        backend,
        { sessionId: id(STACK_ORCHESTRATOR), confirmed: true, confirmedFingerprint: ORCHESTRATOR_FINGERPRINT },
        [
          { sessionId: id(CHILD_RUNNING), confirmed: true, confirmedFingerprint: CHILD_RUNNING_FINGERPRINT },
          { sessionId: id(CHILD_ENDED_WITH_WORKTREE), confirmed: true, confirmedFingerprint: CHILD_ENDED_FINGERPRINT },
        ],
      );
    });
    it("closes no child when the session's own close fails, and shows the session's failure", () => {
      // Given — the orchestrator's close cannot end one of its tabs
      const backend = theDrawerOver({
        ...STACK_SCENARIO,
        closeScripts: {
          [id(STACK_ORCHESTRATOR)]: aCloseThatFails([
            {
              kind: CloseSessionStepKind.TERMINATE,
              subject: "tmux:tddy-orchestrator-review",
              state: CloseSessionStepState.FAILED,
              message: "tab still running 10s after SIGTERM",
              host: "",
            },
          ]),
        },
      });
      close.clickClose(id(STACK_ORCHESTRATOR));
      close.chooseCloseChildrenToo();
      close.expectDialogShows("packages/tddy-service/proto/session.proto");

      // When
      close.confirm();

      // Then — only the orchestrator was asked to close, and its failure is reported
      close.expectFailureNoticeShows(
        id(STACK_ORCHESTRATOR),
        "tmux:tddy-orchestrator-review",
        "tab still running 10s after SIGTERM",
      );
      close.expectCloseRequests(backend, [
        { sessionId: id(STACK_ORCHESTRATOR), confirmed: true, confirmedFingerprint: ORCHESTRATOR_FINGERPRINT },
      ]);
    });
  });

  describe("while a close runs", () => {
    it("shows the row's mini spinner in place of its ×", () => {
      // Given
      theDrawerOver({
        sessions: [LIVEKIT_FIX],
        closeScripts: { [id(LIVEKIT_FIX)]: aCloseThatRunsUntilReleased() },
      });

      // When
      close.clickClose(id(LIVEKIT_FIX));

      // Then
      close.expectClosing(id(LIVEKIT_FIX));
    });

    it("takes the spinner away once the close ends", () => {
      // Given
      const backend = theDrawerOver({
        sessions: [LIVEKIT_FIX],
        closeScripts: { [id(LIVEKIT_FIX)]: aCloseThatRunsUntilReleased() },
      });
      close.clickClose(id(LIVEKIT_FIX));
      close.expectClosing(id(LIVEKIT_FIX));

      // When
      close.releaseClose(backend, id(LIVEKIT_FIX));

      // Then
      close.expectNotClosing(id(LIVEKIT_FIX));
    });

    it("sends another row's close right away, while the first is still running", () => {
      // Given
      const backend = theDrawerOver({
        sessions: [LIVEKIT_FIX, OSC52_COPY],
        closeScripts: { [id(LIVEKIT_FIX)]: aCloseThatRunsUntilReleased("close-task-livekit") },
      });
      close.clickClose(id(LIVEKIT_FIX));
      close.expectClosing(id(LIVEKIT_FIX));

      // When
      close.clickClose(id(OSC52_COPY));

      // Then
      close.expectClosesRequested(backend, [
        { sessionId: id(LIVEKIT_FIX), confirmed: false },
        { sessionId: id(OSC52_COPY), confirmed: false },
      ]);
    });

    it("shows a spinner on each row whose close is running", () => {
      // Given
      theDrawerOver({
        sessions: [LIVEKIT_FIX, OSC52_COPY],
        closeScripts: {
          [id(LIVEKIT_FIX)]: aCloseThatRunsUntilReleased("close-task-livekit"),
          [id(OSC52_COPY)]: aCloseThatRunsUntilReleased("close-task-osc52"),
        },
      });
      close.clickClose(id(LIVEKIT_FIX));

      // When
      close.clickClose(id(OSC52_COPY));

      // Then
      close.expectClosing(id(LIVEKIT_FIX));
      close.expectClosing(id(OSC52_COPY));
    });
  });

  describe("a close the daemon stops to ask again", () => {
    const GREW_SCENARIO: SessionServiceScenario = {
      sessions: [DRAWER_CLOSE],
      closePreviews: { [id(DRAWER_CLOSE)]: DIRTY_PREVIEW },
      closeScripts: { [id(DRAWER_CLOSE)]: [aCloseThatAsksAgain(PREVIEW_THAT_GREW)] },
    };

    it("re-opens the dialog with the new list", () => {
      // Given
      theDrawerOver(GREW_SCENARIO);
      close.clickClose(id(DRAWER_CLOSE));

      // When
      close.confirm();

      // Then
      close.expectDialogOpen();
      close.expectWorktreeSectionShows(0, "packages/tddy-session-web/src/closeSessionNotices.ts");
    });

    it("confirms the new list with its own fingerprint", () => {
      // Given
      const backend = theDrawerOver(GREW_SCENARIO);
      close.clickClose(id(DRAWER_CLOSE));
      close.confirm();
      close.expectWorktreeSectionShows(0, "packages/tddy-session-web/src/closeSessionNotices.ts");

      // When
      close.confirm();

      // Then
      close.expectCloseRequests(backend, [
        { sessionId: id(DRAWER_CLOSE), confirmed: true, confirmedFingerprint: DIRTY_FINGERPRINT },
        { sessionId: id(DRAWER_CLOSE), confirmed: true, confirmedFingerprint: GREW_FINGERPRINT },
      ]);
    });
  });

  describe("a close whose steps failed", () => {
    it("names the failed step, its message and its host in the drawer notices", () => {
      // Given — the review tmux tab on the target would not end, so nothing was removed
      theDrawerOver({
        sessions: [LIVEKIT_FIX],
        closeScripts: {
          [id(LIVEKIT_FIX)]: aCloseThatFails([
            {
              kind: CloseSessionStepKind.TERMINATE,
              subject: "tmux:tddy-4b1d9e2a-review",
              state: CloseSessionStepState.FAILED,
              message: "tab still running 10s after SIGTERM",
              host: "dev-box-3",
            },
          ]),
        },
      });

      // When
      close.clickClose(id(LIVEKIT_FIX));

      // Then
      close.expectFailureNoticeShows(
        id(LIVEKIT_FIX),
        "tmux:tddy-4b1d9e2a-review",
        "tab still running 10s after SIGTERM",
        "dev-box-3",
      );
    });
  });
});
