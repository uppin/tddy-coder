/**
 * Acceptance tests: the session inspector tabs and the terminal tab strip say why a load or an
 * action failed instead of doing nothing.
 *
 * Each of these surfaces used to end its RPCs in `.catch(() => {})`, a `console.debug`, or an early
 * `return` when no client reached the host — so a Remove that the daemon refused left the row in
 * place with no word, and a list that failed to load looked like an empty one. Every failure now
 * lands beside what failed (the row, the form, the list), in the daemon's own words, and a list that
 * failed to load is never shown as an empty list.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { Code, ConnectError, createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { SessionVncTab } from "../../src/components/sessions/SessionVncTab";
import { SessionScreenSharingTab } from "../../src/components/sessions/SessionScreenSharingTab";
import { SessionFilesTab } from "../../src/components/sessions/SessionFilesTab";
import { SessionWorktreeTab } from "../../src/components/sessions/SessionWorktreeTab";
import { SessionToolsTab } from "../../src/components/sessions/SessionToolsTab";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { SessionRuntime } from "../../src/components/sessions/SessionRuntime";
import { Protocol } from "../../src/gen/screen_sharing_pb";
import { SessionFilesService } from "../../src/gen/session_files_pb";
import {
  CalculateWorktreeSizeResponseSchema,
  WorktreeRowSchema,
  WorktreeService,
  WorktreeSizeStatus,
  WorktreeStatsEventSchema,
} from "../../src/gen/worktree_pb";
import { TerminalSessionService } from "../../src/gen/terminal_session_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { UploadProgressProvider } from "../../src/rpc/uploadProgress";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aSessionFilesServiceBackend } from "../support/rpc/sessionFilesServiceBackend";
import {
  aSessionServiceBackend,
  type SessionServiceBackend,
} from "../support/rpc/daemonSessionHostBackend";
import { sessionsDrawerPage as drawer } from "../support/pages/sessionsDrawerPage";
import { sessionFilesTabPage as files } from "../support/pages/sessionFilesTabPage";
import { sessionWorktreeTabPage as worktree } from "../support/pages/sessionWorktreeTabPage";
import { sessionTerminalTabsPage as terminals } from "../support/pages/sessionTerminalTabsPage";
import { sessionTabsErrorsPage as errors } from "../support/pages/sessionTabsErrorsPage";

const SESSION_ID = "tabs-errors-session-0000-0000-0000-000000000001";
const SESSION_TOKEN = "fake-token";

/** What a daemon refusal looks like on the wire. */
const refusal = (message: string, code: Code = Code.FailedPrecondition) => new ConnectError(message, code);

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", SESSION_TOKEN);
});

// ---------------------------------------------------------------------------
// VNC tab
// ---------------------------------------------------------------------------

const VNC_TARGET = { id: "vnc-t-1", label: "Dev Box", host: "10.0.0.5", port: 5900 };

function aVncTab(overrides: Partial<React.ComponentProps<typeof SessionVncTab>> = {}) {
  return (
    <SessionVncTab
      sessionId={SESSION_ID}
      sessionToken={SESSION_TOKEN}
      room={null}
      onListVncTargets={cy.stub().resolves([VNC_TARGET])}
      onAddVncTarget={cy.stub().resolves({ ...VNC_TARGET, id: "vnc-t-2" })}
      onRemoveVncTarget={cy.stub().resolves()}
      onUnlockVncVault={cy.stub().resolves()}
      onStartVncStream={cy.stub().rejects(refusal("unused"))}
      onStopVncStream={cy.stub().resolves()}
      {...overrides}
    />
  );
}

function fillVncAddForm(fields: { label: string; host: string; password?: string }) {
  drawer.vncAddLabel().type(fields.label);
  drawer.vncAddHost().type(fields.host);
  if (fields.password) drawer.vncAddPassword().type(fields.password);
  drawer.vncAddSubmit().click();
}

describe("VNC tab", () => {
  it("shows why the target list failed to load", () => {
    // Given
    cy.mount(aVncTab({ onListVncTargets: cy.stub().rejects(refusal("vnc store unreadable")) }));

    // Then
    errors.vncListError().should("contain.text", "vnc store unreadable");
  });

  it("shows why adding a target failed, and keeps what was typed", () => {
    // Given
    cy.mount(aVncTab({ onAddVncTarget: cy.stub().rejects(refusal("host is not reachable")) }));
    drawer.vncTargetRow(VNC_TARGET.id).should("exist");

    // When
    fillVncAddForm({ label: "Lab", host: "10.9.9.9" });

    // Then
    errors.vncAddError().should("contain.text", "host is not reachable");
    drawer.vncAddHost().should("have.value", "10.9.9.9");
  });

  it("shows why unlocking the vault failed when adding a target with a password", () => {
    // Given
    cy.mount(aVncTab({ onUnlockVncVault: cy.stub().rejects(refusal("wrong passphrase", Code.PermissionDenied)) }));
    drawer.vncTargetRow(VNC_TARGET.id).should("exist");

    // When
    fillVncAddForm({ label: "Lab", host: "10.9.9.9", password: "s3cret" });
    drawer.vncPassphraseInput().type("not-it");
    drawer.vncPassphraseConfirm().click();

    // Then
    errors.vncAddError().should("contain.text", "wrong passphrase");
  });

  it("shows why starting a stream failed on that target's row", () => {
    // Given
    cy.mount(aVncTab({ onStartVncStream: cy.stub().rejects(refusal("bridge failed to start")) }));

    // When
    drawer.vncStartBtn(VNC_TARGET.id).click();

    // Then
    errors.vncStartError(VNC_TARGET.id).should("contain.text", "bridge failed to start");
  });

  it("shows why removing a target failed, and keeps its row", () => {
    // Given
    cy.mount(aVncTab({ onRemoveVncTarget: cy.stub().rejects(refusal("target is streaming")) }));

    // When
    drawer.vncRemoveBtn(VNC_TARGET.id).click();

    // Then
    errors.vncRemoveError(VNC_TARGET.id).should("contain.text", "target is streaming");
    drawer.vncTargetRow(VNC_TARGET.id).should("exist");
  });
});

// ---------------------------------------------------------------------------
// Screen Sharing tab
// ---------------------------------------------------------------------------

const SS_TARGET = {
  id: "ss-t-1",
  label: "Office PC",
  host: "10.0.0.7",
  port: 3389,
  protocol: Protocol.RDP,
  username: "dev",
};

function aScreenSharingTab(overrides: Partial<React.ComponentProps<typeof SessionScreenSharingTab>> = {}) {
  return (
    <SessionScreenSharingTab
      sessionId={SESSION_ID}
      sessionToken={SESSION_TOKEN}
      room={null}
      onListTargets={cy.stub().resolves({ targets: [SS_TARGET], vaultLocked: false })}
      onAddTarget={cy.stub().resolves(SS_TARGET)}
      onRemoveTarget={cy.stub().resolves()}
      onStartStream={cy.stub().rejects(refusal("unused"))}
      onStopStream={cy.stub().resolves()}
      {...overrides}
    />
  );
}

describe("Screen Sharing tab", () => {
  it("shows why the target list failed to load", () => {
    // Given
    cy.mount(aScreenSharingTab({ onListTargets: cy.stub().rejects(refusal("credential store unreadable")) }));

    // Then
    errors.screenSharingListError().should("contain.text", "credential store unreadable");
  });

  it("shows why removing a target failed, and keeps its row", () => {
    // Given
    cy.mount(aScreenSharingTab({ onRemoveTarget: cy.stub().rejects(refusal("target is streaming")) }));

    // When
    drawer.screenSharingRemoveBtn(SS_TARGET.id).click();

    // Then
    errors.screenSharingRemoveError(SS_TARGET.id).should("contain.text", "target is streaming");
    drawer.screenSharingTargetRow(SS_TARGET.id).should("exist");
  });
});

// ---------------------------------------------------------------------------
// Files tab
// ---------------------------------------------------------------------------

const UPLOAD = {
  uploadId: "upload-1",
  fileName: "report.pdf",
  hostPath: `/srv/sessions/${SESSION_ID}/uploads/upload-1/report.pdf`,
  sizeBytes: 2_000n,
  uploadedAtMs: 1_700_000_000_000n,
};

function mountFilesTab(configure: (b: ReturnType<typeof aSessionFilesServiceBackend>["backend"]) => void = () => {}) {
  const { backend } = aSessionFilesServiceBackend({ uploads: [UPLOAD] });
  configure(backend);
  const client = createClient(SessionFilesService, backend.transport());
  mountWithRpc(
    <SessionFilesTab
      client={client}
      sessionToken={SESSION_TOKEN}
      sessionId={SESSION_ID}
      onInsertPath={cy.stub()}
      onCloseInspector={cy.stub()}
    />,
    backend,
  );
}

describe("Files tab", () => {
  it("shows why the upload list failed to load, never an empty list", () => {
    // Given
    mountFilesTab((b) =>
      b.failWith(SessionFilesService.method.listSessionUploads, Code.Internal, "uploads dir unreadable"),
    );

    // Then
    errors.filesError().should("contain.text", "uploads dir unreadable");
    files.empty().should("not.exist");
  });

  it("shows why deleting an upload failed on its row, and keeps the row", () => {
    // Given
    mountFilesTab((b) =>
      b.failWith(SessionFilesService.method.deleteSessionUpload, Code.FailedPrecondition, "file is in use"),
    );

    // When
    files.delete(UPLOAD.fileName).click();
    files.confirmDelete(UPLOAD.fileName).click();

    // Then
    errors.uploadDeleteError(UPLOAD.fileName).should("contain.text", "file is in use");
    files.row(UPLOAD.fileName).should("exist");
  });

  it("says the host is not connected when no client reaches it", () => {
    // Given / When
    cy.mount(
      <SessionFilesTab
        client={null}
        sessionToken={SESSION_TOKEN}
        sessionId={SESSION_ID}
        onInsertPath={cy.stub()}
        onCloseInspector={cy.stub()}
      />,
    );

    // Then
    errors.filesError().should("contain.text", "Not connected");
    files.empty().should("not.exist");
  });
});

// ---------------------------------------------------------------------------
// Worktree tab
// ---------------------------------------------------------------------------

const PROJECT_ID = "proj-tabs-errors";
const REPO_PATH = "/repos/demo/.worktrees/feat-errors";

const CACHED_ROW = {
  path: REPO_PATH,
  branchLabel: "feature/errors",
  sizeStatus: WorktreeSizeStatus.CACHED,
  diskBytes: 1024n,
  changedFiles: 1,
};

function mountWorktreeTab(backend: SessionServiceBackend | null) {
  const client = backend ? createClient(WorktreeService, backend.transport()) : null;
  const tab = (
    <SessionWorktreeTab
      client={client}
      sessionToken={SESSION_TOKEN}
      projectId={PROJECT_ID}
      sessionId={SESSION_ID}
      repoPath={REPO_PATH}
    />
  );
  if (backend) mountWithRpc(tab, backend);
  else cy.mount(tab);
}

describe("Worktree tab", () => {
  it("shows why the stats stream failed instead of loading forever", () => {
    // Given
    const backend = aSessionServiceBackend({ worktreeStatsSnapshot: [CACHED_ROW] });
    backend.failWith(WorktreeService.method.streamWorktreeStats, Code.Internal, "stats walker crashed");

    // When
    mountWorktreeTab(backend);

    // Then
    errors.worktreeError().should("contain.text", "stats walker crashed");
  });

  it("shows why recalculating the size failed", () => {
    // Given
    const backend = aSessionServiceBackend({ worktreeStatsSnapshot: [CACHED_ROW] });
    backend.failWith(WorktreeService.method.calculateWorktreeSize, Code.ResourceExhausted, "size queue is full");
    mountWorktreeTab(backend);

    // When
    worktree.refresh().click();

    // Then
    errors.worktreeError().should("contain.text", "size queue is full");
  });

  it("shows why clearing the worktree failed", () => {
    // Given
    const backend = aSessionServiceBackend({ worktreeStatsSnapshot: [CACHED_ROW] });
    backend.failWith(WorktreeService.method.cleanWorktree, Code.FailedPrecondition, "git clean failed");
    mountWorktreeTab(backend);

    // When
    worktree.clear().click();
    worktree.confirmClear().click();

    // Then
    errors.worktreeError().should("contain.text", "git clean failed");
  });

  it("shows why deleting the worktree failed", () => {
    // Given
    const backend = aSessionServiceBackend({ worktreeStatsSnapshot: [CACHED_ROW] });
    backend.failWith(WorktreeService.method.removeWorktree, Code.FailedPrecondition, "worktree is locked");
    mountWorktreeTab(backend);

    // When
    worktree.delete().click();
    worktree.confirmDelete().click();

    // Then
    errors.worktreeError().should("contain.text", "worktree is locked");
  });

  it("shows why restoring a missing worktree failed", () => {
    // Given the snapshot carries no row for this session's worktree — it is missing
    const backend = aSessionServiceBackend({ worktreeStatsSnapshot: [] });
    backend.failWith(WorktreeService.method.restoreSessionWorktree, Code.NotFound, "branch is gone");
    mountWorktreeTab(backend);

    // When
    worktree.restore().click();

    // Then
    errors.worktreeError().should("contain.text", "branch is gone");
  });

  it("a successful recalculation does not hide a dropped stats stream", () => {
    // Given a stream that delivers the row and then drops, and a recalculation that fails once
    let recalculations = 0;
    const backend = anInMemoryRpcBackend().implement(WorktreeService, {
      streamWorktreeStats: async function* () {
        yield create(WorktreeStatsEventSchema, { snapshot: [create(WorktreeRowSchema, CACHED_ROW)] });
        throw refusal("stats walker crashed", Code.Internal);
      },
      calculateWorktreeSize: async () => {
        recalculations += 1;
        if (recalculations === 1) throw refusal("size queue is full", Code.ResourceExhausted);
        return create(CalculateWorktreeSizeResponseSchema, {});
      },
    });
    const client = createClient(WorktreeService, backend.transport());
    mountWithRpc(
      <SessionWorktreeTab
        client={client}
        sessionToken={SESSION_TOKEN}
        projectId={PROJECT_ID}
        sessionId={SESSION_ID}
        repoPath={REPO_PATH}
      />,
      backend,
    );
    errors.worktreeError().should("contain.text", "stats walker crashed");
    worktree.refresh().click();
    errors.worktreeError().should("contain.text", "size queue is full");

    // When the next recalculation succeeds
    worktree.refresh().click();

    // Then the stream is still reported dead
    errors.worktreeError().should("contain.text", "stats walker crashed");
  });

  it("says the host is not connected when no client reaches it", () => {
    // Given / When
    mountWorktreeTab(null);

    // Then
    errors.worktreeError().should("contain.text", "Not connected");
  });
});

// ---------------------------------------------------------------------------
// Tools tab
// ---------------------------------------------------------------------------

const SHELL_TOOL = { name: "Shell", description: "Run a shell command", inputSchemaJson: "{}" };

function aToolsTab(overrides: Partial<React.ComponentProps<typeof SessionToolsTab>> = {}) {
  return (
    <SessionToolsTab
      sessionId={SESSION_ID}
      onListExecTools={cy.stub().resolves([SHELL_TOOL])}
      onListSessionToolCalls={cy.stub().resolves([])}
      onExecuteTool={cy.stub().resolves({ resultJson: '{"ok":true}', isError: false, errorMessage: "" })}
      {...overrides}
    />
  );
}

describe("Tools tab", () => {
  it("shows why the tool catalog failed to load", () => {
    // Given
    cy.mount(aToolsTab({ onListExecTools: cy.stub().rejects(refusal("exec tools unavailable", Code.Unavailable)) }));

    // Then
    errors.toolListError().should("contain.text", "exec tools unavailable");
  });

  it("shows why the call log failed to load, never an empty log", () => {
    // Given
    cy.mount(aToolsTab({ onListSessionToolCalls: cy.stub().rejects(refusal("call log unreadable")) }));

    // Then
    errors.toolCallLogError().should("contain.text", "call log unreadable");
    errors.toolCallLog().should("not.contain.text", "No tool calls recorded");
  });

  it("shows why refreshing the call log after an invoke failed, and keeps the result", () => {
    // Given the log loads on mount, then fails when refreshed after the invoke
    const onListSessionToolCalls = cy.stub();
    onListSessionToolCalls.onFirstCall().resolves([]);
    onListSessionToolCalls.rejects(refusal("call log unreadable"));
    cy.mount(aToolsTab({ onListSessionToolCalls }));

    // When
    errors.invokeTool();

    // Then
    errors.toolInvokeResult().should("contain.text", '{"ok":true}');
    errors.toolCallLogError().should("contain.text", "call log unreadable");
  });
});

// ---------------------------------------------------------------------------
// Terminal tab strip
// ---------------------------------------------------------------------------

const TERMINAL_SESSION = {
  sessionId: SESSION_ID,
  createdAt: "2026-10-23T09:00:00Z",
  status: "active",
  repoPath: "/home/dev/feature-errors",
  pid: 90002,
  isActive: true,
  projectId: "proj-term-errors",
  daemonInstanceId: "local",
  pendingElicitation: false,
};

/** A host-served backend (empty `livekitRoom`), so terminal RPCs reach the in-memory daemon. */
function aTerminalBackend(terminalIds: string[] = []): SessionServiceBackend {
  return aSessionServiceBackend({
    sessions: [TERMINAL_SESSION],
    connectSession: () => ({ livekitRoom: "", livekitUrl: "", livekitServerIdentity: "" }),
    terminals: terminalIds.map((terminalId) => ({ terminalId })),
  });
}

function attachTerminalSession(backend: SessionServiceBackend) {
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  drawer.drawerItem(TERMINAL_SESSION.sessionId).click();
  terminals.tabs().should("exist");
}

describe("Terminal tab strip", () => {
  it("shows why listing the session's terminals failed", () => {
    // Given
    const backend = aTerminalBackend();
    backend.failWith(TerminalSessionService.method.listTerminalSessions, Code.Internal, "pty registry unreadable");

    // When
    attachTerminalSession(backend);

    // Then
    errors.terminalError().should("contain.text", "pty registry unreadable");
  });

  it("shows why opening a new terminal failed, and opens no tab", () => {
    // Given
    const backend = aTerminalBackend();
    backend.failWith(TerminalSessionService.method.startTerminalSession, Code.ResourceExhausted, "too many terminals");
    attachTerminalSession(backend);

    // When
    terminals.newTab().click();
    terminals.newTabOption("bash").click();

    // Then
    errors.terminalError().should("contain.text", "too many terminals");
    terminals.tab("bash-1").should("not.exist");
  });

  it("shows why stopping a closed terminal failed", () => {
    // Given
    const backend = aTerminalBackend(["bash-1"]);
    backend.failWith(TerminalSessionService.method.stopTerminalSession, Code.FailedPrecondition, "terminal is held by another tab");
    attachTerminalSession(backend);

    // When
    terminals.tabClose("bash-1").click();

    // Then
    errors.terminalError().should("contain.text", "terminal is held by another tab");
  });

  it("says the host is not connected when a terminal is opened with no client to start it", () => {
    // Given a runtime whose session has no connection yet, so no terminal client exists
    cy.mount(
      <UploadProgressProvider>
        <div style={{ width: 900, height: 500, position: "relative" }}>
          <SessionRuntime
            runtime={{
              sessionId: SESSION_ID,
              attached: true,
              bytesIn: 0,
              bytesOut: 0,
              lastDataReceivedAt: null,
            }}
            focused
            sessionToken={SESSION_TOKEN}
          />
        </div>
      </UploadProgressProvider>,
    );

    // When
    terminals.newTab().click();
    terminals.newTabOption("bash").click();

    // Then
    errors.terminalError().should("contain.text", "Not connected to the selected host");
  });
});
