import React from "react";
import { Code } from "@connectrpc/connect";
import { Room } from "livekit-client";
import { ConnectionScreen } from "../../src/components/ConnectionScreen";
import { ConnectionService, Signal } from "../../src/gen/connection_pb";
import type { DaemonHost } from "../../src/lib/participantRole";
import { SelectedDaemonProvider } from "../../src/rpc/selectedDaemon";
import {
  aConnectionServiceBackend,
  connectionServiceProjectIdCollisionScenario,
  DAEMON_LOCAL,
  DAEMON_PEER,
  COLLISION_PROJECT_ID,
  type ConnectionServiceBackend,
  type ConnectionServiceScenario,
  type DaemonEntry,
} from "../support/rpc/connectionServiceBackend";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { byTestId, TEST_IDS } from "../support/testIds";
import { connectionPage } from "../support/pages/connectionPage";

// ---------------------------------------------------------------------------
// Mount helper
// ---------------------------------------------------------------------------

/** Default single local daemon — mirrors `aConnectionServiceBackend`'s own default. */
const DEFAULT_DAEMONS: DaemonEntry[] = [{ instanceId: "local", label: "local (this daemon)", isLocal: true }];

function daemonHostsFrom(daemons: DaemonEntry[] | undefined): DaemonHost[] {
  return (daemons ?? DEFAULT_DAEMONS).map((d) => ({ instanceId: d.instanceId, label: d.label }));
}

/**
 * Builds an `aConnectionServiceBackend(scenario)` and mounts `ConnectionScreen` wrapped in a
 * `SelectedDaemonProvider` routed to it (`ConnectionService` is daemon-level RPC — see
 * `../support/rpc/connectionServiceBackend`'s doc comment). The provider's `daemons` list is
 * derived from `scenario.daemons` so `useDaemonClient` always has a selected daemon and a real
 * client to build.
 */
function mountConnectionScreen(
  scenario: ConnectionServiceScenario = {},
  props: React.ComponentProps<typeof ConnectionScreen> = {},
): ConnectionServiceBackend {
  const backend = aConnectionServiceBackend(scenario);
  mountWithRecordingLiveKitRpc(
    <SelectedDaemonProvider room={new Room()} daemons={daemonHostsFrom(scenario.daemons)}>
      <ConnectionScreen {...props} />
    </SelectedDaemonProvider>,
    backend,
  );
  return backend;
}

// ---------------------------------------------------------------------------
// Session / project constant builders (Partial<SessionEntry> objects)
// ---------------------------------------------------------------------------

const ACTIVE_SESSION = {
  sessionId: "session-active-1",
  createdAt: "2026-03-21T10:00:00Z",
  status: "active",
  repoPath: "/home/dev/project",
  pid: 12345,
  isActive: true,
  projectId: "proj-1",
  daemonInstanceId: "",
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
  daemonInstanceId: "",
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
  daemonInstanceId: "",
};
const PROJ_ORDER_ACTIVE_OLD = {
  sessionId: "proj-order-active-old",
  createdAt: "2026-03-21T08:00:00Z",
  status: "active",
  repoPath: "/home/dev/project",
  pid: 402,
  isActive: true,
  projectId: "proj-1",
  daemonInstanceId: "",
};
const PROJ_ORDER_INACTIVE_NEW = {
  sessionId: "proj-order-inactive-new",
  createdAt: "2026-03-21T11:00:00Z",
  status: "exited",
  repoPath: "/home/dev/project",
  pid: 0,
  isActive: false,
  projectId: "proj-1",
  daemonInstanceId: "",
};
const PROJ_ORDER_INACTIVE_OLD = {
  sessionId: "proj-order-inactive-old",
  createdAt: "2026-03-21T07:00:00Z",
  status: "exited",
  repoPath: "/home/dev/project",
  pid: 0,
  isActive: false,
  projectId: "proj-1",
  daemonInstanceId: "",
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

const PROJECT_ID = "proj-1";

/** Row key when ListProjects tags the project with the local eligible daemon (multi-host mocks). */
const PROJECT_ROW_MULTI_HOST = `${PROJECT_ID}__${DAEMON_LOCAL.instanceId}`;

const SESSION_WITH_HOST = {
  sessionId: "session-host-1",
  createdAt: "2026-03-28T10:00:00Z",
  status: "active",
  repoPath: "/home/dev/project",
  pid: 12345,
  isActive: true,
  projectId: "proj-1",
  daemonInstanceId: "workstation-1",
};

/** `ACTIVE_SESSION` with daemon matching multi-host ListProjects + ListEligibleDaemons local row. */
const ACTIVE_SESSION_ON_LOCAL_ELIGIBLE_DAEMON = {
  ...ACTIVE_SESSION,
  daemonInstanceId: DAEMON_LOCAL.instanceId,
};

const COLLISION_SESSION_WS = {
  sessionId: "collision-sess-ws",
  createdAt: "2026-04-10T10:00:00Z",
  status: "active",
  repoPath: "/home/ws/dup",
  pid: 60001,
  isActive: true,
  projectId: COLLISION_PROJECT_ID,
  daemonInstanceId: DAEMON_LOCAL.instanceId,
};

const COLLISION_SESSION_SRV = {
  sessionId: "collision-sess-srv",
  createdAt: "2026-04-10T11:00:00Z",
  status: "active",
  repoPath: "/srv/dup",
  pid: 60002,
  isActive: true,
  projectId: COLLISION_PROJECT_ID,
  daemonInstanceId: DAEMON_PEER.instanceId,
};

/** Extended ListSessions payload — acceptance / TUI parity (workflow columns). */
const STATUS_PARITY_SESSION_V1 = {
  sessionId: "session-status-parity-1",
  createdAt: "2026-03-21T10:00:00Z",
  status: "active",
  repoPath: "/home/dev/project",
  pid: 12345,
  isActive: true,
  projectId: "proj-1",
  daemonInstanceId: "",
  workflowGoal: "acceptance-tests",
  workflowState: "Red",
  elapsedDisplay: "1m 2s",
  agent: "claude",
  model: "sonnet-4",
};

const STATUS_PARITY_SESSION_V2 = {
  ...STATUS_PARITY_SESSION_V1,
  workflowState: "Green",
  elapsedDisplay: "3m 0s",
};

const SESSION_MULTI_A = {
  sessionId: "multi-session-aaaaaaaa-bbbb-cccc-dddd-eeee11111111",
  createdAt: "2026-03-21T10:00:00Z",
  status: "active",
  repoPath: "/home/dev/project",
  pid: 70001,
  isActive: true,
  projectId: "proj-1",
  daemonInstanceId: "",
};

const SESSION_MULTI_B = {
  sessionId: "multi-session-bbbbbbbb-aaaa-cccc-dddd-ffff22222222",
  createdAt: "2026-03-21T11:00:00Z",
  status: "active",
  repoPath: "/home/dev/project",
  pid: 70002,
  isActive: true,
  projectId: "proj-1",
  daemonInstanceId: "",
};

/** Derives a per-session LiveKit room/identity from the requested sessionId (multi-session tests). */
function connectSessionPerSessionId(sessionId: string) {
  return {
    livekitRoom: `room-${sessionId}`,
    livekitServerIdentity: `server-${sessionId.slice(0, 8)}`,
  };
}

// ---------------------------------------------------------------------------

describe("ConnectionScreen terminal chrome — status dot menu", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
  });

  it("ConnectionScreen connected daemon session exposes Terminate from the dot menu", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");
    const backend = mountConnectionScreen({ sessions: [ACTIVE_SESSION] });
    cy.window().then((win) => {
      cy.stub(win, "confirm").returns(true).as("confirmTerminate");
    });

    // When
    connectionPage.connectBtn(ACTIVE_SESSION.sessionId).click();

    // Then
    connectionPage.terminalContainer().should("exist");
    // Token fetch uses standalone chrome; after the JWT arrives Ghostty mounts and replaces the tree — wait for LiveKit UI before clicking the dot (avoids detached DOM).
    connectionPage.statusDot().first().should("be.visible").and("have.attr", "data-connection-status");
    connectionPage.livekitStatus().should("not.be.visible");
    connectionPage.statusDot().first().should("be.visible").click();
    connectionPage.disconnectMenuItem().should("be.visible");
    connectionPage.terminateMenuItem().should("be.visible");
    connectionPage.terminateMenuItem().click({ force: true });
    cy.get("@confirmTerminate").should("have.been.calledOnce");
    cy.wrap(backend).should((b) => {
      expect(b.signalCalls).to.deep.equal([{ sessionId: ACTIVE_SESSION.sessionId, signal: Signal.SIGTERM }]);
    });
  });

  it("cancelling Terminate confirmation does not call SignalSession", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");
    const backend = mountConnectionScreen({ sessions: [ACTIVE_SESSION] });
    cy.window().then((win) => {
      cy.stub(win, "confirm").returns(false);
    });

    // When
    connectionPage.connectBtn(ACTIVE_SESSION.sessionId).click();
    connectionPage.terminalContainer().should("exist");
    connectionPage.statusDot().first().should("be.visible").and("have.attr", "data-connection-status");
    connectionPage.livekitStatus().should("not.be.visible");
    connectionPage.statusDot().first().should("be.visible").click();
    connectionPage.disconnectMenuItem().should("be.visible");
    connectionPage.terminateMenuItem().should("be.visible");
    connectionPage.terminateMenuItem().click({ force: true });

    // Then
    cy.wrap(backend).should((b) => {
      expect(b.signalCalls, "Terminate cancel must not hit SignalSession").to.deep.equal([]);
    });
  });
});

describe("ConnectionScreen connected participants", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
  });

  it("does not render presence panel without common room config", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");

    // When
    mountConnectionScreen({ sessions: [ACTIVE_SESSION] });

    // Then
    connectionPage.sessionsTable(PROJECT_ID).should("exist");
    byTestId(TEST_IDS.connectedParticipantsPanel).should("not.exist");
  });

  it("renders presence panel when livekit URL and common room are provided", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");

    // When
    mountConnectionScreen(
      { sessions: [ACTIVE_SESSION] },
      { livekitUrl: "ws://127.0.0.1:7880", commonRoom: "tddy-lobby" },
    );

    // Then
    byTestId(TEST_IDS.connectedParticipantsPanel, { timeout: 5000 }).should("exist");
    byTestId(TEST_IDS.participantList, { timeout: 5000 }).should("exist");
    byTestId(TEST_IDS.participantList, { timeout: 20000 }).should(($el) => {
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
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");

    // When
    mountConnectionScreen({ sessions: [ACTIVE_SESSION] });

    // Then
    connectionPage.signalDropdown(ACTIVE_SESSION.sessionId).should("exist");
  });

  it("hides signal dropdown for inactive sessions", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");

    // When
    mountConnectionScreen({ sessions: [ACTIVE_SESSION, INACTIVE_SESSION] });

    // Then
    connectionPage.sessionsTable(PROJECT_ID).should("exist");
    connectionPage.signalDropdown(ACTIVE_SESSION.sessionId).should("exist");
    connectionPage.signalDropdown(INACTIVE_SESSION.sessionId).should("not.exist");
  });

  it("signal dropdown shows three options", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");
    mountConnectionScreen({ sessions: [ACTIVE_SESSION] });

    // When
    connectionPage.openSignalDropdown(ACTIVE_SESSION.sessionId);

    // Then
    connectionPage.signalSigint(ACTIVE_SESSION.sessionId)
      .should("exist")
      .and("contain.text", "Interrupt");
    connectionPage.signalSigterm(ACTIVE_SESSION.sessionId)
      .should("exist")
      .and("contain.text", "Terminate");
    connectionPage.signalSigkill(ACTIVE_SESSION.sessionId)
      .should("exist")
      .and("contain.text", "Kill");
  });

  it("clicking interrupt calls signal session rpc with sigint", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");
    const backend = mountConnectionScreen({ sessions: [ACTIVE_SESSION] });

    // When
    connectionPage.openSignalDropdown(ACTIVE_SESSION.sessionId);
    connectionPage.signalSigint(ACTIVE_SESSION.sessionId).click();

    // Then
    cy.wrap(backend).should((b) => {
      expect(b.signalCalls).to.deep.equal([{ sessionId: ACTIVE_SESSION.sessionId, signal: Signal.SIGINT }]);
    });
  });

  it("clicking terminate calls signal session rpc with sigterm", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");
    const backend = mountConnectionScreen({ sessions: [ACTIVE_SESSION] });

    // When
    connectionPage.openSignalDropdown(ACTIVE_SESSION.sessionId);
    connectionPage.signalSigterm(ACTIVE_SESSION.sessionId).click();

    // Then
    cy.wrap(backend).should((b) => {
      expect(b.signalCalls).to.deep.equal([{ sessionId: ACTIVE_SESSION.sessionId, signal: Signal.SIGTERM }]);
    });
  });

  it("clicking kill calls signal session rpc with sigkill", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");
    const backend = mountConnectionScreen({ sessions: [ACTIVE_SESSION] });

    // When
    connectionPage.openSignalDropdown(ACTIVE_SESSION.sessionId);
    connectionPage.signalSigkill(ACTIVE_SESSION.sessionId).click();

    // Then
    cy.wrap(backend).should((b) => {
      expect(b.signalCalls).to.deep.equal([{ sessionId: ACTIVE_SESSION.sessionId, signal: Signal.SIGKILL }]);
    });
  });

  it("dropdown closes after action", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");
    mountConnectionScreen({ sessions: [ACTIVE_SESSION] });

    // When
    connectionPage.openSignalDropdown(ACTIVE_SESSION.sessionId);
    connectionPage.signalSigint(ACTIVE_SESSION.sessionId).click();

    // Then
    connectionPage.signalMenu(ACTIVE_SESSION.sessionId).should("not.exist");
  });

  it("shows error when signal rpc fails", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");
    mountConnectionScreen({
      sessions: [ACTIVE_SESSION],
      signalSessionError: { code: Code.FailedPrecondition, message: "Process not alive" },
    });

    // When
    connectionPage.openSignalDropdown(ACTIVE_SESSION.sessionId);
    connectionPage.signalSigint(ACTIVE_SESSION.sessionId).click();

    // Then
    connectionPage.connectionError().should("exist");
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
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");

    // When
    mountConnectionScreen({ sessions: sessionsForDeleteSuite });

    // Then
    connectionPage.sessionsTable(PROJECT_ID).should("exist");
    connectionPage.connectBtn(ACTIVE_SESSION.sessionId).should("exist");
    connectionPage.connectBtn(ORPHAN_ACTIVE_SESSION.sessionId).should("exist");
    connectionPage.deleteSessionBtn(ACTIVE_SESSION.sessionId).should("exist");
    connectionPage.deleteSessionBtn(ORPHAN_ACTIVE_SESSION.sessionId).should("exist");
    connectionPage.deleteSessionBtn(INACTIVE_SESSION.sessionId).should("exist");
    connectionPage.deleteSessionBtn(ORPHAN_INACTIVE_SESSION.sessionId).should("exist");
    connectionPage.orphanTable().should("exist");
  });

  it("clicking_delete_confirmed_calls_delete_session_rpc", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");
    const backend = mountConnectionScreen({ sessions: sessionsForDeleteSuite });
    cy.window().then((win) => {
      cy.stub(win, "confirm").returns(true);
    });

    // When
    connectionPage.deleteSessionBtn(INACTIVE_SESSION.sessionId).click();

    // Then
    cy.wrap(backend).should((b) => {
      expect(b.deletedSessionIds).to.include(INACTIVE_SESSION.sessionId);
    });
  });
});

describe("ConnectionScreen bulk session selection and delete (acceptance)", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
  });

  it("header select all checks every row checkbox in a project session table", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");
    mountConnectionScreen({ sessions: [ACTIVE_SESSION, INACTIVE_SESSION] });

    // When
    connectionPage.sessionsTable(PROJECT_ID).within(() => {
      connectionPage.selectAll(PROJECT_ID).click();
    });

    // Then
    connectionPage.sessionsTable(PROJECT_ID).within(() => {
      connectionPage.sessionRowCheckbox(ACTIVE_SESSION.sessionId).should("be.checked");
      connectionPage.sessionRowCheckbox(INACTIVE_SESSION.sessionId).should("be.checked");
      connectionPage.selectAll(PROJECT_ID).should("be.checked");
    });
  });

  it("header checkbox shows indeterminate when selection is partial", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");
    mountConnectionScreen({ sessions: [ACTIVE_SESSION, INACTIVE_SESSION] });

    // When
    connectionPage.sessionsTable(PROJECT_ID).within(() => {
      connectionPage.sessionRowCheckbox(ACTIVE_SESSION.sessionId).click();
    });

    // Then
    connectionPage.sessionsTable(PROJECT_ID).within(() => {
      connectionPage.sessionRowCheckbox(INACTIVE_SESSION.sessionId).should("not.be.checked");
      connectionPage.selectAll(PROJECT_ID).should(($el) => {
        const el = $el.get(0) as HTMLInputElement;
        expect(el.indeterminate, "header checkbox indeterminate when partial").to.be.true;
      });
    });
  });

  it("bulk delete confirms once and calls deleteSession once per selected id", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");
    const backend = mountConnectionScreen({ sessions: [ACTIVE_SESSION, INACTIVE_SESSION] });

    // When
    connectionPage.sessionsTable(PROJECT_ID).within(() => {
      connectionPage.selectAll(PROJECT_ID).click();
    });
    cy.window().then((win) => {
      cy.stub(win, "confirm")
        .callsFake((msg: string) => {
          expect(msg).to.include("2");
          expect(msg.toLowerCase()).to.include("delete");
          return true;
        })
        .as("bulkDeleteConfirm");
    });
    // NOTE: bulk-delete-selected-<tableKey> does NOT match connectionPage.bulkDeleteButton()
    // (which uses bulk-delete-button-<id>). Keep raw selector until component is fixed.
    cy.get(`[data-testid="bulk-delete-selected-${PROJECT_ID}"]`).click();

    // Then
    cy.get("@bulkDeleteConfirm").should("have.been.calledOnce");
    cy.wrap(backend).should((b) => {
      expect(b.deletedSessionIds.slice().sort()).to.deep.equal(
        [ACTIVE_SESSION.sessionId, INACTIVE_SESSION.sessionId].sort(),
      );
      expect(b.callsTo(ConnectionService.method.listSessions).length).to.be.at.least(2);
    });
  });

  it("bulk delete does not call DeleteSession when user cancels confirm", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");
    const backend = mountConnectionScreen({ sessions: [ACTIVE_SESSION, INACTIVE_SESSION] });

    // When
    connectionPage.sessionsTable(PROJECT_ID).within(() => {
      connectionPage.selectAll(PROJECT_ID).click();
    });
    cy.window().then((win) => {
      cy.stub(win, "confirm").returns(false);
    });
    // NOTE: bulk-delete-selected-<tableKey> does NOT match connectionPage.bulkDeleteButton()
    // (which uses bulk-delete-button-<id>). Keep raw selector until component is fixed.
    cy.get(`[data-testid="bulk-delete-selected-${PROJECT_ID}"]`).click();

    // Then
    cy.wrap(backend).should((b) => {
      expect(b.deletedSessionIds, "cancelled bulk delete must not call DeleteSession").to.deep.equal([]);
    });
  });

  it("Delete selected is disabled when no rows are selected", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");

    // When
    mountConnectionScreen({ sessions: [ACTIVE_SESSION, INACTIVE_SESSION] });

    // Then
    // NOTE: bulk-delete-selected-<tableKey> does NOT match connectionPage.bulkDeleteButton()
    // (which uses bulk-delete-button-<id>). Keep raw selector until component is fixed.
    cy.get(`[data-testid="bulk-delete-selected-${PROJECT_ID}"]`, { timeout: 5000 }).should(
      "be.disabled",
    );
  });

  it("orphan table bulk selection does not clear project table selection", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");
    mountConnectionScreen({ sessions: [ACTIVE_SESSION, INACTIVE_SESSION, ORPHAN_ACTIVE_SESSION] });

    // When
    connectionPage.sessionsTable(PROJECT_ID).within(() => {
      connectionPage.selectAll(PROJECT_ID).click();
    });
    connectionPage.orphanTable().within(() => {
      connectionPage.sessionRowCheckbox(ORPHAN_ACTIVE_SESSION.sessionId).click();
    });

    // Then
    connectionPage.sessionsTable(PROJECT_ID).within(() => {
      connectionPage.sessionRowCheckbox(ACTIVE_SESSION.sessionId).should("be.checked");
      connectionPage.sessionRowCheckbox(INACTIVE_SESSION.sessionId).should("be.checked");
    });
    connectionPage.orphanTable().within(() => {
      connectionPage.sessionRowCheckbox(ORPHAN_ACTIVE_SESSION.sessionId).should("be.checked");
    });
  });
});

describe("ConnectionScreen session table ordering", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
  });

  it("ConnectionScreen — sorts project sessions with active on top then by createdAt descending", () => {
    // Given
    /** Deliberately wrong: inactive rows first; active newest last — sorted UI must fix order. */
    window.localStorage.setItem("tddy_session_token", "fake-token");
    mountConnectionScreen({
      sessions: [
        PROJ_ORDER_INACTIVE_OLD,
        PROJ_ORDER_ACTIVE_OLD,
        PROJ_ORDER_INACTIVE_NEW,
        PROJ_ORDER_ACTIVE_NEW,
      ],
    });

    // Then
    connectionPage.sessionsTable(PROJECT_ID).should("exist");
    const expectedOrder = [
      PROJ_ORDER_ACTIVE_NEW.sessionId,
      PROJ_ORDER_ACTIVE_OLD.sessionId,
      PROJ_ORDER_INACTIVE_NEW.sessionId,
      PROJ_ORDER_INACTIVE_OLD.sessionId,
    ];
    expectedOrder.forEach((sessionId, index) => {
      connectionPage.sessionsTable(PROJECT_ID).find("tbody tr")
        .eq(index)
        .find(`[data-testid="connect-${sessionId}"], [data-testid="resume-${sessionId}"]`)
        .should("exist");
    });
  });

  it("ConnectionScreen — sorts orphan sessions with active on top then by createdAt descending", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");
    mountConnectionScreen({
      sessions: [
        ORPH_ORDER_INACTIVE_OLD,
        ORPH_ORDER_ACTIVE_OLD,
        ORPH_ORDER_INACTIVE_NEW,
        ORPH_ORDER_ACTIVE_NEW,
      ],
    });

    // Then
    connectionPage.orphanTable().should("exist");
    const expectedOrder = [
      ORPH_ORDER_ACTIVE_NEW.sessionId,
      ORPH_ORDER_ACTIVE_OLD.sessionId,
      ORPH_ORDER_INACTIVE_NEW.sessionId,
      ORPH_ORDER_INACTIVE_OLD.sessionId,
    ];
    expectedOrder.forEach((sessionId, index) => {
      connectionPage.orphanTable().find("tbody tr")
        .eq(index)
        .find(`[data-testid="connect-${sessionId}"], [data-testid="resume-${sessionId}"]`)
        .should("exist");
    });
  });
});

describe("ConnectionScreen session status (TUI parity)", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
  });

  it("connection_screen_renders_status_columns_from_extended_session_entry", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");

    // When
    mountConnectionScreen({ sessions: [STATUS_PARITY_SESSION_V1] });

    // Then
    const sid = STATUS_PARITY_SESSION_V1.sessionId;
    byTestId(`session-row-workflow-goal-${sid}`, { timeout: 8000 })
      .scrollIntoView()
      .should("be.visible")
      .and("contain.text", "acceptance-tests");
    byTestId(`session-row-workflow-state-${sid}`).should("contain.text", "Red");
    byTestId(`session-row-agent-${sid}`).should("contain.text", "claude");
    byTestId(`session-row-model-${sid}`).should("contain.text", "sonnet-4");
  });

  it("connection_screen_updates_row_when_live_status_payload_changes", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");
    let call = 0;
    mountConnectionScreen({
      sessions: [STATUS_PARITY_SESSION_V1],
      listSessionsFactory: () => {
        call += 1;
        return [call === 1 ? STATUS_PARITY_SESSION_V1 : STATUS_PARITY_SESSION_V2];
      },
    });

    // Then
    const sid = STATUS_PARITY_SESSION_V1.sessionId;
    byTestId(`session-row-workflow-state-${sid}`, { timeout: 8000 }).should(
      "contain.text",
      "Red",
    );
    cy.wait(5500);
    byTestId(`session-row-workflow-state-${sid}`, { timeout: 8000 }).should("contain.text", "Green");
    byTestId(`session-row-elapsed-${sid}`).should("contain.text", "3m 0s");
  });
});

describe("ConnectionScreen ListAgents backend select (acceptance)", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
  });

  it("connection_screen_backend_select_uses_list_agents", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");

    // When
    mountConnectionScreen({
      sessions: [],
      agents: [
        { id: "agent-one", label: "Agent One" },
        { id: "agent-two", label: "Agent Two" },
      ],
    });

    // Then
    connectionPage.backendSelect(PROJECT_ID).should("exist");
    connectionPage.backendSelect(PROJECT_ID).find("option").should("have.length", 2);
    connectionPage.backendSelect(PROJECT_ID).find("option").eq(0).should("have.value", "agent-one");
    connectionPage.backendSelect(PROJECT_ID).find("option").eq(1).should("have.value", "agent-two");
    connectionPage.backendSelect(PROJECT_ID).find("option[value='claude']").should("not.exist");
    connectionPage.backendSelect(PROJECT_ID).find("option[value='cursor']").should("not.exist");

    // Given — re-mount with different agents
    mountConnectionScreen({ sessions: [], agents: [{ id: "gamma-only", label: "Gamma" }] });

    // Then
    connectionPage.backendSelect(PROJECT_ID).find("option").should("have.length", 1);
    connectionPage.backendSelect(PROJECT_ID).should("have.value", "gamma-only");
  });
});

describe("ConnectionScreen multi-host daemon selection", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
  });

  it("renders a Host dropdown per project populated from ListEligibleDaemons", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");

    // When
    mountConnectionScreen({
      sessions: [ACTIVE_SESSION_ON_LOCAL_ELIGIBLE_DAEMON],
      daemons: [DAEMON_LOCAL, DAEMON_PEER],
    });

    // Then
    connectionPage.hostSelect(PROJECT_ROW_MULTI_HOST).should("exist");
    connectionPage.hostSelect(PROJECT_ROW_MULTI_HOST).find("option").should("have.length", 2);
    connectionPage.hostSelect(PROJECT_ROW_MULTI_HOST).find("option").eq(0).should("contain.text", DAEMON_LOCAL.label);
    connectionPage.hostSelect(PROJECT_ROW_MULTI_HOST).find("option").eq(1).should("contain.text", DAEMON_PEER.label);
  });

  /** PRD: local daemon row first in the Host dropdown even when the RPC returns peers before the local row. */
  it("web_connection_screen_host_dropdown_lists_multiple_daemons", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");

    // When
    mountConnectionScreen({
      sessions: [ACTIVE_SESSION_ON_LOCAL_ELIGIBLE_DAEMON],
      daemons: [
        { instanceId: DAEMON_PEER.instanceId, label: DAEMON_PEER.label, isLocal: false },
        { instanceId: DAEMON_LOCAL.instanceId, label: DAEMON_LOCAL.label, isLocal: true },
      ],
    });

    // Then
    connectionPage.hostSelect(PROJECT_ROW_MULTI_HOST).should("exist");
    connectionPage.hostSelect(PROJECT_ROW_MULTI_HOST).find("option").should("have.length", 2);
    connectionPage.hostSelect(PROJECT_ROW_MULTI_HOST).find("option").eq(0).should("contain.text", DAEMON_LOCAL.label);
    connectionPage.hostSelect(PROJECT_ROW_MULTI_HOST).find("option").eq(1).should("contain.text", DAEMON_PEER.label);
  });

  it("defaults Host dropdown to the local daemon", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");

    // When
    mountConnectionScreen({
      sessions: [ACTIVE_SESSION_ON_LOCAL_ELIGIBLE_DAEMON],
      daemons: [DAEMON_LOCAL, DAEMON_PEER],
    });

    // Then
    connectionPage.hostSelect(PROJECT_ROW_MULTI_HOST).should("have.value", DAEMON_LOCAL.instanceId);
  });

  it("web_start_session_includes_recipe_field", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");
    const backend = mountConnectionScreen({ sessions: [], daemons: [DAEMON_LOCAL, DAEMON_PEER] });

    // When
    byTestId(`recipe-select-${PROJECT_ROW_MULTI_HOST}`, { timeout: 5000 }).select("bugfix");
    connectionPage.startSession(PROJECT_ROW_MULTI_HOST).click();

    // Then
    cy.wrap(backend).should((b) => {
      const calls = b.callsTo(ConnectionService.method.startSession);
      expect(calls, "StartSession called once").to.have.length(1);
      expect(calls[0]!.recipe).to.eq("bugfix");
    });
  });

  it("sends daemonInstanceId in StartSession request", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");
    const backend = mountConnectionScreen({ sessions: [], daemons: [DAEMON_LOCAL, DAEMON_PEER] });

    // When
    connectionPage.hostSelect(PROJECT_ROW_MULTI_HOST).select(DAEMON_PEER.instanceId);
    connectionPage.startSession(PROJECT_ROW_MULTI_HOST).click();

    // Then
    cy.wrap(backend).should((b) => {
      const calls = b.callsTo(ConnectionService.method.startSession);
      expect(calls, "StartSession called once").to.have.length(1);
      expect(calls[0]!.daemonInstanceId).to.eq(DAEMON_PEER.instanceId);
    });
  });

  it("shows Host column in session tables", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");

    // When
    mountConnectionScreen({ sessions: [SESSION_WITH_HOST], daemons: [DAEMON_LOCAL, DAEMON_PEER] });

    // Then
    connectionPage.sessionsTable(PROJECT_ROW_MULTI_HOST)
      .find("th")
      .should("contain.text", "Host");
    connectionPage.sessionsTable(PROJECT_ROW_MULTI_HOST).find("tbody tr")
      .first()
      .should("contain.text", "workstation-1");
  });

  it("renders single option when only one daemon is available", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");

    // When
    mountConnectionScreen({
      sessions: [ACTIVE_SESSION_ON_LOCAL_ELIGIBLE_DAEMON],
      daemons: [DAEMON_LOCAL],
    });

    // Then
    connectionPage.hostSelect(PROJECT_ROW_MULTI_HOST).should("exist");
    connectionPage.hostSelect(PROJECT_ROW_MULTI_HOST).find("option").should("have.length", 1);
  });
});

describe("ConnectionScreen multi-daemon project collision (acceptance)", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
  });

  it("renders_separate_accordions_per_project_and_host_when_project_id_collides", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");

    // When
    mountConnectionScreen(
      connectionServiceProjectIdCollisionScenario([COLLISION_SESSION_WS, COLLISION_SESSION_SRV]),
    );

    // Then
    byTestId(`project-accordion-${COLLISION_PROJECT_ID}__${DAEMON_LOCAL.instanceId}`, { timeout: 8000 }).should("exist");
    byTestId(`project-accordion-${COLLISION_PROJECT_ID}__${DAEMON_PEER.instanceId}`).should("exist");

    byTestId(`sessions-table-${COLLISION_PROJECT_ID}__${DAEMON_LOCAL.instanceId}`).within(() => {
      connectionPage.connectBtn(COLLISION_SESSION_WS.sessionId).should("exist");
      connectionPage.connectBtn(COLLISION_SESSION_SRV.sessionId).should("not.exist");
    });
    byTestId(`sessions-table-${COLLISION_PROJECT_ID}__${DAEMON_PEER.instanceId}`).within(() => {
      connectionPage.connectBtn(COLLISION_SESSION_SRV.sessionId).should("exist");
      connectionPage.connectBtn(COLLISION_SESSION_WS.sessionId).should("not.exist");
    });
  });
});

describe("ConnectionScreen — reconnect vs new-session presentation", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
  });

  it("Resume inactive session shows floating overlay without pushing /terminal onto history", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");
    mountConnectionScreen({
      sessions: [INACTIVE_SESSION],
      resumeSession: { sessionId: INACTIVE_SESSION.sessionId },
    });
    cy.window().then((win) => {
      cy.spy(win.history, "pushState").as("historyPush");
    });

    // When
    byTestId(`resume-${INACTIVE_SESSION.sessionId}`, { timeout: 5000 }).click();

    // Then
    connectionPage.reconnectOverlay().should("be.visible");
    cy.get("@historyPush").should("not.have.been.called");
  });

  it("Connect active session opens floating overlay without pushing /terminal onto history", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");
    mountConnectionScreen({ sessions: [ACTIVE_SESSION] });
    cy.window().then((win) => {
      cy.spy(win.history, "pushState").as("historyPush");
    });

    // When
    connectionPage.connectBtn(ACTIVE_SESSION.sessionId).click();

    // Then
    connectionPage.reconnectOverlay().should("be.visible");
    connectionPage.terminalContainer({ timeout: 15000 }).should("exist");
    cy.get("@historyPush").should("not.have.been.called");
  });
});

describe("ConnectionScreen — pending elicitation indicator", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
  });

  it("ConnectionScreen_shows_elicitation_indicator_when_pending_elicitation_true", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");

    // When
    mountConnectionScreen({ sessions: [SESSION_PENDING_ELICITATION] });

    // Then
    connectionPage.sessionsTable(PROJECT_ID).should("exist");
    connectionPage.connectBtn(SESSION_PENDING_ELICITATION.sessionId)
      .closest("tr")
      .should("have.attr", "data-pending-elicitation", "true");
  });

  it("ConnectionScreen_hides_elicitation_indicator_when_pending_elicitation_false", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");

    // When
    mountConnectionScreen({ sessions: [SESSION_WITHOUT_ELICITATION] });

    // Then
    connectionPage.connectBtn(SESSION_WITHOUT_ELICITATION.sessionId)
      .closest("tr")
      .should("have.attr", "data-pending-elicitation", "false");
  });
});

// ---------------------------------------------------------------------------
// Multi-session concurrent attachments (acceptance — PRD: Connection screen N≥1)
// ---------------------------------------------------------------------------

describe("ConnectionScreen — multi-session attachments (acceptance)", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
  });

  it("ConnectionScreen — two concurrent connects: two distinct attachment roots (mocked RPC)", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");
    mountConnectionScreen({
      sessions: [SESSION_MULTI_A, SESSION_MULTI_B],
      connectSession: connectSessionPerSessionId,
    });

    // When
    connectionPage.connectBtn(SESSION_MULTI_A.sessionId, { timeout: 8000 }).click();
    connectionPage.terminalContainer().should("exist");
    connectionPage.connectBtn(SESSION_MULTI_B.sessionId).click();

    // Then
    connectionPage.attachedTerminal(SESSION_MULTI_A.sessionId).should("exist");
    connectionPage.attachedTerminal(SESSION_MULTI_B.sessionId).should("exist");
  });

  it("ConnectionScreen — disconnect first leaves second terminal (mocked)", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");
    mountConnectionScreen({
      sessions: [SESSION_MULTI_A, SESSION_MULTI_B],
      connectSession: connectSessionPerSessionId,
    });

    // When
    connectionPage.connectBtn(SESSION_MULTI_A.sessionId, { timeout: 8000 }).click();
    connectionPage.attachedTerminal(SESSION_MULTI_A.sessionId).should("exist");
    connectionPage.connectBtn(SESSION_MULTI_B.sessionId).click();
    connectionPage.attachedTerminal(SESSION_MULTI_B.sessionId).should("exist");
    connectionPage.attachedTerminal(SESSION_MULTI_A.sessionId)
      .find(`[data-testid='${TEST_IDS.connectionStatusDot}']`, { timeout: 20000 })
      .should("have.length.at.least", 1)
      .first()
      .scrollIntoView()
      .click({ force: true });
    connectionPage.disconnectMenuItem().first().click({ force: true });

    // Then
    connectionPage.attachedTerminal(SESSION_MULTI_B.sessionId).should("exist");
    connectionPage.attachedTerminal(SESSION_MULTI_B.sessionId)
      .find(`[data-testid='${TEST_IDS.connectionStatusDot}']`, { timeout: 20000 })
      .first()
      .should("exist");
  });

  it("ConnectionScreen — Start New Session with another terminal open adds attachment (mocked)", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");
    const newSessionId = "multi-session-new-start-cccc-dddd-eeee-ffffffffffff";
    mountConnectionScreen({
      sessions: [SESSION_MULTI_A, SESSION_MULTI_B],
      connectSession: connectSessionPerSessionId,
      startSession: {
        sessionId: newSessionId,
        livekitRoom: `room-${newSessionId}`,
        livekitServerIdentity: "server-new",
      },
    });

    // When
    // Cross-test DOM/layout residue from a floating terminal overlay in a prior test in this
    // describe block can leave the connect button visually covered at the moment of click — force,
    // matching the established convention for post-attach interactions elsewhere in this block.
    connectionPage.connectBtn(SESSION_MULTI_A.sessionId, { timeout: 8000 }).click({ force: true });
    connectionPage.attachedTerminal(SESSION_MULTI_A.sessionId).should("exist");
    connectionPage.startSession(PROJECT_ID).click({ force: true });

    // Then
    connectionPage.attachedTerminal(SESSION_MULTI_A.sessionId, { timeout: 20000 }).should("exist");
    connectionPage.attachedTerminal(newSessionId).should("exist");
  });

  it("ConnectionScreen — inactive ListSessions clears only matching attachment (mocked)", () => {
    // Given
    window.localStorage.setItem("tddy_session_token", "fake-token");
    let poll = 0;
    mountConnectionScreen({
      sessions: [SESSION_MULTI_A, SESSION_MULTI_B],
      listSessionsFactory: () => {
        poll += 1;
        if (poll === 1) {
          return [SESSION_MULTI_A, SESSION_MULTI_B];
        }
        return [
          { ...SESSION_MULTI_A, isActive: false, status: "exited", pid: 0 },
          SESSION_MULTI_B,
        ];
      },
      connectSession: connectSessionPerSessionId,
    });

    // When
    connectionPage.connectBtn(SESSION_MULTI_A.sessionId, { timeout: 8000 }).click();
    connectionPage.attachedTerminal(SESSION_MULTI_A.sessionId).should("exist");
    connectionPage.connectBtn(SESSION_MULTI_B.sessionId).click();
    connectionPage.attachedTerminal(SESSION_MULTI_B.sessionId).should("exist");
    cy.wait(5500);

    // Then
    connectionPage.attachedTerminal(SESSION_MULTI_B.sessionId).should("exist");
    connectionPage.attachedTerminal(SESSION_MULTI_A.sessionId).should("not.exist");
  });
});
