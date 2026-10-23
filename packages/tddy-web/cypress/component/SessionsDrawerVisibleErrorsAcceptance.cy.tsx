/**
 * Acceptance tests: the sessions drawer, a session's runtime and its terminal say why something did
 * not work, instead of doing nothing.
 *
 * Every one of these used to be silent: a click handler that returned early when no client reached
 * the session's host, a `.catch` that only `console.debug`'d, an attach error held in state and
 * never rendered, a failed list poll that left the drawer looking empty, a terminal that dropped
 * keystrokes the daemon refused. Each failure now lands next to the surface it happened on — the
 * list, the selected session's pane, the bulk-delete row, the child pane, the claim overlay, the
 * terminal — and clears on the next success.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { Code, ConnectError, createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  DeleteSessionResponseSchema,
  ListSessionsResponseSchema,
  SessionService,
  SessionEntrySchema,
} from "../../src/gen/session_pb";
import {
  ClaimTerminalControlResponseSchema,
  SendTerminalInputResponseSchema,
  TerminalSessionService,
} from "../../src/gen/terminal_session_pb";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { REFRESH_INTERVAL_MS } from "../../src/components/sessions/sessionManager";
import { SessionRuntime } from "../../src/components/sessions/SessionRuntime";
import { GrpcSessionTerminal } from "../../src/components/sessions/GrpcSessionTerminal";
import { TerminalControlOverlay } from "../../src/components/sessions/TerminalControlOverlay";
import { useTerminalControl } from "../../src/components/sessions/useTerminalControl";
import { SelectedDaemonProvider } from "../../src/rpc/selectedDaemon";
import { AuthProvider } from "../../src/hooks/authProvider";
import { UploadProgressProvider } from "../../src/rpc/uploadProgress";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aSessionServiceBackend, type SessionServiceBackend } from "../support/rpc/daemonSessionHostBackend";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionConnection } from "../support/rpc/sessionConnections";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { sessionTerminalTabsPage as tabs } from "../support/pages/sessionTerminalTabsPage";
import { sessionsVisibleErrorsPage as errors } from "../support/pages/sessionsVisibleErrorsPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/** A host the drawer lists sessions for but no wire reaches — it is not in the daemon directory. */
const UNREACHABLE_HOST = "server-2";

function aSession(id: string, overrides: Record<string, unknown> = {}) {
  return {
    sessionId: `visible-errors-${id}-0000-0000-0000-000000000000`,
    createdAt: "2026-10-23T12:00:00Z",
    status: "exited",
    repoPath: `/home/dev/${id}`,
    pid: 0,
    isActive: false,
    projectId: "proj-visible-errors",
    daemonInstanceId: "",
    workflowGoal: `Session ${id}`,
    pendingElicitation: false,
    ...overrides,
  };
}

const DORMANT = aSession("dormant");
const DORMANT_ELSEWHERE = aSession("dormant-elsewhere", { daemonInstanceId: UNREACHABLE_HOST });
const LIVE_ELSEWHERE = aSession("live-elsewhere", {
  daemonInstanceId: UNREACHABLE_HOST,
  status: "active",
  isActive: true,
  pid: 4242,
});
const SESSION_A = aSession("aaaa");
const SESSION_B = aSession("bbbb");

/** Answers ConnectSession over the host's own wire (no room), so attaches settle in-memory. */
const HOST_SERVED = { livekitRoom: "", livekitUrl: "", livekitServerIdentity: "" };

function mountDrawer(backend: SessionServiceBackend) {
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
}

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

// ---------------------------------------------------------------------------
// Session list
// ---------------------------------------------------------------------------

describe("the session list", () => {
  it("shows why the list could not be loaded", () => {
    // Given
    const backend = aSessionServiceBackend({ sessions: [DORMANT] }).failWith(
      SessionService.method.listSessions,
      Code.Internal,
      "session store is unreadable",
    );

    // When
    mountDrawer(backend);

    // Then
    errors.listLoadError().should("contain.text", "session store is unreadable");
  });

  it("keeps the last good list beside the error when a later poll fails", () => {
    // Given a list that answers once, then fails
    cy.clock(Date.now(), ["setInterval", "clearInterval"]);
    let calls = 0;
    const backend = aSessionServiceBackend({ sessions: [DORMANT] }).onUnary(
      SessionService.method.listSessions,
      () => {
        calls += 1;
        if (calls > 1) throw new ConnectError("daemon is restarting", Code.Unavailable);
        return create(ListSessionsResponseSchema, { sessions: [create(SessionEntrySchema, DORMANT)] });
      },
    );

    mountDrawer(backend);
    sessionsDrawerPage.drawerItem(DORMANT.sessionId).should("exist");

    // When the next poll runs
    cy.tick(REFRESH_INTERVAL_MS);

    // Then
    errors.listLoadError().should("contain.text", "daemon is restarting");
    sessionsDrawerPage.drawerItem(DORMANT.sessionId).should("exist");
  });

  it("clears the list error once a later poll succeeds", () => {
    // Given a list that fails once, then answers
    cy.clock(Date.now(), ["setInterval", "clearInterval"]);
    let calls = 0;
    const backend = aSessionServiceBackend({ sessions: [DORMANT] }).onUnary(
      SessionService.method.listSessions,
      () => {
        calls += 1;
        if (calls === 1) throw new ConnectError("daemon is busy", Code.Unavailable);
        return create(ListSessionsResponseSchema, { sessions: [create(SessionEntrySchema, DORMANT)] });
      },
    );
    mountDrawer(backend);
    errors.listLoadError().should("contain.text", "daemon is busy");

    // When the next poll runs
    cy.tick(REFRESH_INTERVAL_MS);

    // Then
    sessionsDrawerPage.drawerItem(DORMANT.sessionId).should("exist");
    errors.listLoadError().should("not.exist");
  });

  it("says the host is not connected instead of showing an empty list", () => {
    // Given a selected host that no wire reaches
    cy.mount(
      <AuthProvider>
        <SelectedDaemonProvider room={null} daemons={[{ instanceId: "local", label: "local (this daemon)" }]}>
          <SessionsDrawerScreen />
        </SelectedDaemonProvider>
      </AuthProvider>,
    );

    // Then
    errors.listLoadError().should("contain.text", "Not connected to local");
  });
});

// ---------------------------------------------------------------------------
// The selected session's pane actions
// ---------------------------------------------------------------------------

describe("the selected session's actions", () => {
  it("says the owning host is not connected when resuming a session on it", () => {
    // Given a dormant session on a host no wire reaches
    mountDrawer(aSessionServiceBackend({ sessions: [DORMANT_ELSEWHERE] }));
    sessionsDrawerPage.drawerItem(DORMANT_ELSEWHERE.sessionId).click();

    // When
    sessionsDrawerPage.inspectorResumeBtn(DORMANT_ELSEWHERE.sessionId).click();

    // Then
    errors.paneError().should("contain.text", "Resume failed").and("contain.text", `Not connected to ${UNREACHABLE_HOST}`);
  });

  it("says the owning host is not connected when deleting a session on it", () => {
    // Given
    mountDrawer(aSessionServiceBackend({ sessions: [DORMANT_ELSEWHERE] }));
    sessionsDrawerPage.drawerItem(DORMANT_ELSEWHERE.sessionId).click();

    // When
    sessionsDrawerPage.inspectorDeleteBtn(DORMANT_ELSEWHERE.sessionId).click();
    sessionsDrawerPage.inspectorDeleteConfirm(DORMANT_ELSEWHERE.sessionId).click();

    // Then
    errors.paneError().should("contain.text", "Delete failed").and("contain.text", `Not connected to ${UNREACHABLE_HOST}`);
  });

  it("says the owning host is not connected when attaching a live session on it", () => {
    // Given
    mountDrawer(aSessionServiceBackend({ sessions: [LIVE_ELSEWHERE] }));

    // When
    sessionsDrawerPage.drawerItem(LIVE_ELSEWHERE.sessionId).click();

    // Then
    errors.paneError().should("contain.text", "Attach failed").and("contain.text", `Not connected to ${UNREACHABLE_HOST}`);
  });

  it("says the owning host is not connected when terminating a live session on it", () => {
    // Given
    mountDrawer(aSessionServiceBackend({ sessions: [LIVE_ELSEWHERE] }));
    sessionsDrawerPage.drawerItem(LIVE_ELSEWHERE.sessionId).click();
    sessionsDrawerPage.inspectorToggle().click();

    // When
    sessionsDrawerPage.inspectorTerminateBtn(LIVE_ELSEWHERE.sessionId).click();

    // Then
    errors.paneError().should("contain.text", "Terminate failed").and("contain.text", `Not connected to ${UNREACHABLE_HOST}`);
  });

  it("shows the daemon's reason when a resume fails", () => {
    // Given
    const backend = aSessionServiceBackend({ sessions: [DORMANT] }).failWith(
      SessionService.method.resumeSession,
      Code.FailedPrecondition,
      "the session's worktree was removed",
    );
    mountDrawer(backend);
    sessionsDrawerPage.drawerItem(DORMANT.sessionId).click();

    // When
    sessionsDrawerPage.inspectorResumeBtn(DORMANT.sessionId).click();

    // Then — the daemon's words, without the transport's `[code]`
    errors.paneError().should("have.text", "the session's worktree was removed");
  });

  it("shows the daemon's reason when a delete fails", () => {
    // Given
    const backend = aSessionServiceBackend({ sessions: [DORMANT] }).failWith(
      SessionService.method.deleteSession,
      Code.PermissionDenied,
      "only the session's owner may delete it",
    );
    mountDrawer(backend);
    sessionsDrawerPage.drawerItem(DORMANT.sessionId).click();

    // When
    sessionsDrawerPage.inspectorDeleteBtn(DORMANT.sessionId).click();
    sessionsDrawerPage.inspectorDeleteConfirm(DORMANT.sessionId).click();

    // Then — the daemon's words, without the transport's `[code]`
    errors.paneError().should("have.text", "Permission denied — only the session's owner may delete it");
  });
});

// ---------------------------------------------------------------------------
// Bulk delete
// ---------------------------------------------------------------------------

describe("bulk delete", () => {
  it("names each row that was not deleted, and why", () => {
    // Given B's delete is refused
    const backend = aSessionServiceBackend({ sessions: [SESSION_A, SESSION_B] }).onUnary(
      SessionService.method.deleteSession,
      (req) => {
        if (req.sessionId === SESSION_B.sessionId) {
          throw new ConnectError("session B is still running", Code.FailedPrecondition);
        }
        return create(DeleteSessionResponseSchema, {});
      },
    );
    mountDrawer(backend);

    // When
    errors.bulkDelete([SESSION_A.sessionId, SESSION_B.sessionId]);

    // Then
    errors.bulkDeleteError(SESSION_B.sessionId).should("contain.text", "session B is still running");
    errors.bulkDeleteError(SESSION_A.sessionId).should("not.exist");
  });

  it("names a row whose host no wire reaches, sends its delete nowhere, and still deletes the rows after it", () => {
    // Given a row on a host no wire reaches, ticked before a row on the selected host
    const deleteRequests: string[] = [];
    const backend = aSessionServiceBackend({ sessions: [SESSION_A, DORMANT_ELSEWHERE] }).onUnary(
      SessionService.method.deleteSession,
      (req) => {
        deleteRequests.push(req.sessionId);
        return create(DeleteSessionResponseSchema, {});
      },
    );
    mountDrawer(backend);

    // When
    errors.bulkDelete([DORMANT_ELSEWHERE.sessionId, SESSION_A.sessionId]);

    // Then the unreachable row says why, and only the reachable row's delete was sent
    errors.bulkDeleteError(DORMANT_ELSEWHERE.sessionId).should("contain.text", `Not connected to ${UNREACHABLE_HOST}`);
    cy.wrap(deleteRequests).should("deep.equal", [SESSION_A.sessionId]);
  });

  it("does not report a row's failure as the selected session's", () => {
    // Given A is selected, and B's delete is refused
    const backend = aSessionServiceBackend({ sessions: [SESSION_A, SESSION_B] }).onUnary(
      SessionService.method.deleteSession,
      (req) => {
        if (req.sessionId === SESSION_B.sessionId) {
          throw new ConnectError("session B is still running", Code.FailedPrecondition);
        }
        return create(DeleteSessionResponseSchema, {});
      },
    );
    mountDrawer(backend);
    sessionsDrawerPage.drawerItem(SESSION_A.sessionId).click();

    // When
    errors.bulkDelete([SESSION_B.sessionId]);

    // Then
    errors.bulkDeleteError(SESSION_B.sessionId).should("exist");
    errors.paneError().should("not.exist");
  });
});

// ---------------------------------------------------------------------------
// Spawned child conversations
// ---------------------------------------------------------------------------

const PARENT = aSession("parent", {
  status: "active",
  isActive: true,
  pid: 90001,
  daemonInstanceId: "local",
  recipe: "grill-me",
  sessionType: "cursor-cli",
});
const CHILD = aSession("child", {
  status: "active",
  isActive: true,
  pid: 90002,
  daemonInstanceId: "local",
  orchestratorSessionId: PARENT.sessionId,
});

describe("a spawned child conversation", () => {
  it("shows why the child could not be attached", () => {
    // Given the child's ConnectSession is refused
    const backend = aSessionServiceBackend({
      sessions: [PARENT, CHILD],
      connectSession: (sessionId) => {
        if (sessionId === CHILD.sessionId) throw new ConnectError("child worktree is locked", Code.Aborted);
        return HOST_SERVED;
      },
    });
    mountDrawer(backend);
    sessionsDrawerPage.drawerItem(PARENT.sessionId).click();

    // When
    tabs.childTab(CHILD.sessionId).click();

    // Then — the daemon's words, without the transport's `[code]`
    errors.childAttachError(CHILD.sessionId).should("have.text", "child worktree is locked");
  });

  it("says the host is not connected when there is none to attach the child on", () => {
    // Given a parent runtime with no host connection to attach its child over
    const backend = aSessionServiceBackend({ sessions: [PARENT, CHILD], connectSession: HOST_SERVED });
    const connection = aSessionConnection(PARENT.sessionId).servingOver(backend.transport()).build();
    cy.mount(
      <UploadProgressProvider>
        <div style={{ width: 900, height: 500, position: "relative" }}>
          <SessionRuntime
            runtime={{
              sessionId: PARENT.sessionId,
              attached: true,
              connection,
              bytesIn: 0,
              bytesOut: 0,
              lastDataReceivedAt: null,
            }}
            focused
            sessionToken="fake-token"
            host={null}
            sessions={[create(SessionEntrySchema, PARENT), create(SessionEntrySchema, CHILD)]}
          />
        </div>
      </UploadProgressProvider>,
    );

    // When
    tabs.childTab(CHILD.sessionId).click();

    // Then
    errors.childAttachError(CHILD.sessionId).should("contain.text", "Not connected to local");
  });
});

// ---------------------------------------------------------------------------
// Terminal control
// ---------------------------------------------------------------------------

const CLAIM_SESSION = "visible-errors-claim-0000-0000-0000-000000000000";

/** The runtime's own wiring of the control lease to its overlay, over `backend`. */
function ClaimHarness({ backend }: { backend: ReturnType<typeof anInMemoryRpcBackend> }) {
  const client = React.useMemo(() => createClient(TerminalSessionService, backend.transport()), [backend]);
  const { controlState, claim, claimError } = useTerminalControl(
    { sessionId: CLAIM_SESSION, client },
    "fake-token",
  );
  return (
    <div style={{ width: 600, height: 300, position: "relative" }}>
      <TerminalControlOverlay
        isController={controlState.isController}
        holderScreenId={controlState.holderScreenId}
        onClaim={claim}
        claimError={claimError}
      />
    </div>
  );
}

/** Another screen holds the lease; a steal-claim answers as `steal` says. */
function aControlBackend(steal: () => { granted: boolean; controlToken: string }) {
  return anInMemoryRpcBackend()
    .onUnary(TerminalSessionService.method.claimTerminalControl, (req) =>
      create(
        ClaimTerminalControlResponseSchema,
        req.steal ? steal() : { granted: false, currentHolderScreenId: "other-screen" },
      ),
    )
    .implement(TerminalSessionService, {
      // eslint-disable-next-line require-yield
      async *watchTerminalControl() {
        await new Promise<never>(() => {});
      },
    });
}

describe("claiming the terminal", () => {
  it("shows why a manual claim failed, and keeps the claim button", () => {
    // Given
    const backend = aControlBackend(() => {
      throw new ConnectError("control lease store is unavailable", Code.Unavailable);
    });
    cy.mount(<ClaimHarness backend={backend} />);

    // When
    sessionsDrawerPage.terminalClaimBtn().click();

    // Then
    errors.claimError().should("contain.text", "control lease store is unavailable");
    sessionsDrawerPage.terminalClaimBtn().should("be.visible");
  });

  it("clears the claim error once a later claim succeeds", () => {
    // Given a steal-claim that fails once, then is granted
    let attempts = 0;
    const backend = aControlBackend(() => {
      attempts += 1;
      if (attempts === 1) throw new ConnectError("daemon is busy", Code.Unavailable);
      return { granted: true, controlToken: "lease-2" };
    });
    cy.mount(<ClaimHarness backend={backend} />);
    sessionsDrawerPage.terminalClaimBtn().click();
    errors.claimError().should("contain.text", "daemon is busy");

    // When
    sessionsDrawerPage.terminalClaimBtn().click();

    // Then
    sessionsDrawerPage.terminalControlOverlay().should("not.exist");
    errors.claimError().should("not.exist");
  });
});

// ---------------------------------------------------------------------------
// Terminal input
// ---------------------------------------------------------------------------

const TERMINAL_SESSION = "visible-errors-terminal-0000-0000-0000-000000000000";

/** A terminal whose input the daemon refuses while `refusing()` says so; `sends` counts the inputs. */
function aTerminalBackend(refusing: () => boolean, sends = { count: 0 }) {
  return anInMemoryRpcBackend()
    .onUnary(TerminalSessionService.method.sendTerminalInput, () => {
      sends.count += 1;
      if (refusing()) throw new ConnectError("terminal controlled by another screen", Code.FailedPrecondition);
      return create(SendTerminalInputResponseSchema, {});
    })
    .implement(TerminalSessionService, {
      // eslint-disable-next-line require-yield
      async *streamTerminalOutput() {
        await new Promise<never>(() => {});
      },
    });
}

function mountTerminal(backend: ReturnType<typeof anInMemoryRpcBackend>) {
  const client = createClient(TerminalSessionService, backend.transport());
  cy.mount(
    <UploadProgressProvider>
      <div style={{ height: 400, width: 800, position: "relative" }}>
        <GrpcSessionTerminal
          sessionId={TERMINAL_SESSION}
          sessionToken="fake-token"
          client={client}
          connected={{ sessionId: TERMINAL_SESSION, controlToken: "lease-1" }}
        />
      </div>
    </UploadProgressProvider>,
  );
  errors.readyTerminal();
}

describe("typing into the terminal", () => {
  it("shows one indicator when input is not delivered, however many keys were refused", () => {
    // Given
    const sends = { count: 0 };
    mountTerminal(aTerminalBackend(() => true, sends));

    // When
    errors.typeIntoTerminal("abc");

    // Then every key was sent and refused, and they share one indicator
    cy.wrap(sends).its("count").should("be.gte", 3);
    errors.inputError()
      .should("have.length", 1)
      .and("contain.text", "Input not delivered")
      .and("contain.text", "terminal controlled by another screen");
  });

  it("clears the indicator on the next delivered input", () => {
    // Given input that is refused until the lease comes back
    let refusing = true;
    mountTerminal(aTerminalBackend(() => refusing));
    errors.typeIntoTerminal("a");
    errors.inputError().should("exist");

    // When
    cy.then(() => {
      refusing = false;
    });
    errors.typeIntoTerminal("b");

    // Then
    errors.inputError().should("not.exist");
  });
});

