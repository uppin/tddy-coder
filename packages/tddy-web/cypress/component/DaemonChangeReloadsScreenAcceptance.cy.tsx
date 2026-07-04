/**
 * Acceptance tests: switching the selected daemon must *reload the active screen* — not merely
 * refetch its top-level list. Every daemon-mode screen already re-runs its fetch effect when the
 * daemon changes (the `useDaemonClient` reference changes), but transient view state built up
 * against the previous daemon — the selected session, an open inspector, a live terminal
 * attachment, unsaved create/VM/task UI state — survives the switch, leaving a half-reloaded screen
 * pointed at a daemon it no longer talks to.
 *
 * The contract: changing the selected daemon remounts the daemon-mode screen subtree, giving each
 * screen a fresh lifecycle against the newly selected daemon (state reset, effects re-run).
 *
 * PRD: docs/ft/web/daemon-selector-livekit-rpc.md.
 */

import React, { useEffect } from "react";
import { Room } from "livekit-client";
import type { DaemonHost } from "../../src/lib/participantRole";
import { SelectedDaemonProvider } from "../../src/rpc/selectedDaemon";
import { DaemonSelectorConnected } from "../../src/components/shell/DaemonSelector";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aConnectionServiceBackend } from "../support/rpc/connectionServiceBackend";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { daemonSelectorPage } from "../support/pages/daemonSelectorPage";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";

const DAEMON_A: DaemonHost = { instanceId: "alpha", label: "alpha (this daemon)" };
const DAEMON_B: DaemonHost = { instanceId: "beta", label: "beta (this daemon)" };

// ---------------------------------------------------------------------------
// The reload contract, at the provider level
// ---------------------------------------------------------------------------

/** Reports each fresh mount of the daemon-mode screen subtree via a stable callback. */
function ScreenMountProbe({ onMount }: { onMount: () => void }) {
  useEffect(() => {
    onMount();
  }, [onMount]);
  return <div data-testid="probe-screen">screen</div>;
}

it("remounts the active screen subtree when the selected daemon changes", () => {
  // Given — a daemon-mode screen mounted once for the initially selected daemon
  const onMount = cy.stub().as("screenMount");
  cy.mount(
    <SelectedDaemonProvider room={new Room()} daemons={[DAEMON_A, DAEMON_B]} servingInstanceId="alpha">
      <DaemonSelectorConnected />
      <ScreenMountProbe onMount={onMount} />
    </SelectedDaemonProvider>,
  );
  cy.get("@screenMount").should("have.been.calledOnce");

  // When — the operator switches to a different daemon
  daemonSelectorPage.choose("beta");

  // Then — the screen subtree is reloaded (remounted) for the newly selected daemon
  cy.get("@screenMount").should("have.been.calledTwice");
});

// ---------------------------------------------------------------------------
// The reload contract, applied to a real screen
// ---------------------------------------------------------------------------

const SESSION_ON_ALPHA = {
  sessionId: "reload-aaaaaaaa-0000-0000-0000-000000000001",
  createdAt: "2026-07-01T12:00:00Z",
  status: "active",
  repoPath: "/home/dev/reload-branch",
  pid: 30001,
  isActive: true,
  projectId: "proj-reload-1",
  daemonInstanceId: "",
  workflowGoal: "A session on the first daemon",
  pendingElicitation: false,
};

describe("SessionsDrawerScreen — changing daemon reloads the screen", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("clears the previously inspected session when the daemon changes", () => {
    // Given — a session is selected and its inspector is open, on the first daemon.
    // `connectSession` config mirrors the proven SessionTerminateRefetch harness: an active
    // session's terminal mounts on select and connects to a (deliberately unreachable) LiveKit URL,
    // failing gracefully — without the config it renders down a broken path.
    const backend = aConnectionServiceBackend({
      sessions: [SESSION_ON_ALPHA],
      connectSession: {
        livekitRoom: "room-a",
        livekitUrl: "ws://127.0.0.1:7880",
        livekitServerIdentity: "server",
      },
    });
    mountWithRecordingLiveKitRpc(
      withSelectedDaemon(<SessionsDrawerScreen />, [DAEMON_A, DAEMON_B]),
      backend,
    );
    sessionsDrawerPage.drawerItem(SESSION_ON_ALPHA.sessionId).click();
    sessionsDrawerPage.inspectorToggle().click();
    sessionsDrawerPage.inspectorDrawer().should("have.attr", "data-state", "open");

    // When — the operator switches to a different daemon
    daemonSelectorPage.choose("beta");

    // Then — the screen reloads fresh: no session is selected, so the inspector is gone
    sessionsDrawerPage.inspectorDrawer().should("not.exist");
  });
});
