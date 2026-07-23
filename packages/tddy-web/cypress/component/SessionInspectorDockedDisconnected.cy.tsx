/**
 * Acceptance tests: the Session Inspector docks as the MAIN PANE for a disconnected session,
 * and stays an overlay drawer otherwise. As a consequence, the "Claim terminal" overlay is not
 * rendered for a disconnected session (its focused runtime foreground is suppressed) while the
 * runtime layer stays mounted so background sessions keep streaming.
 *
 * Amendment: docs/ft/web/1-WIP/inspector-docked-when-disconnected.md
 * PRD:       docs/ft/web/session-drawer.md § Session Inspector Drawer
 *
 * Layout ACs (1-3) drive the full SessionsDrawerScreen over the recording LiveKit harness
 * (mirrors SessionInspectorAcceptance). The Claim-terminal ACs (4-5) drive SessionMainPane
 * directly with an explicit runtime + a fake ConnectionService client (mirrors
 * TerminalControlAcceptance / SessionRuntimeStealClaimReattach), since they need a runtime that
 * is still mounted for a session that is / is not disconnected.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  ConnectionService,
  ClaimTerminalControlResponseSchema,
  TerminalControlEventSchema,
} from "../../src/gen/connection_pb";
import type { SessionEntry } from "../../src/gen/connection_pb";
import type { SessionRuntimeState } from "../../src/components/sessions/sessionRuntimeRegistry";
import type { SessionAttachmentState } from "../../src/components/sessions/useSessionAttachment";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { SessionMainPane } from "../../src/components/sessions/SessionMainPane";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aConnectionServiceBackend } from "../support/rpc/connectionServiceBackend";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { sessionsDrawerPage as page } from "../support/pages/sessionsDrawerPage";
import { byTestId, TEST_IDS } from "../support/testIds";

// ---------------------------------------------------------------------------
// Session fixtures
// ---------------------------------------------------------------------------

const CONNECTED_SESSION = {
  sessionId: "docked-connected-aaaaaaaa-0000-0000-0000-000000000000",
  createdAt: "2026-07-23T12:00:00Z",
  status: "active",
  repoPath: "/home/dev/live-branch",
  pid: 20001,
  isActive: true,
  projectId: "proj-docked-1",
  daemonInstanceId: "",
  workflowGoal: "Live session",
  pendingElicitation: false,
};

const DISCONNECTED_SESSION = {
  sessionId: "docked-disconnected-cccccccc-0000-0000-0000-000000000000",
  createdAt: "2026-07-23T10:00:00Z",
  status: "exited",
  repoPath: "/home/dev/old-branch",
  pid: 0,
  isActive: false,
  projectId: "proj-docked-1",
  daemonInstanceId: "",
  workflowGoal: "Old work",
  pendingElicitation: false,
};

// ---------------------------------------------------------------------------
// Fake ConnectionService client for the SessionMainPane-direct runtime tests
// ---------------------------------------------------------------------------

const OTHER_SCREEN = "screen-held-by-another-9999";

/** A ConnectionService whose control lease is held by another screen (so the focused runtime's
 *  auto-claim is denied and the "Claim terminal" CTA WOULD show), with a never-ending terminal
 *  output stream so the runtime mounts and stays stable. */
function aClaimDeniedClient() {
  const backend = anInMemoryRpcBackend().implement(ConnectionService, {
    claimTerminalControl: async () =>
      create(ClaimTerminalControlResponseSchema, {
        granted: false,
        currentHolderScreenId: OTHER_SCREEN,
      }),
    watchTerminalControl: async function* (
      _req: unknown,
      context: { signal: AbortSignal },
    ) {
      yield create(TerminalControlEventSchema, {
        holderScreenId: OTHER_SCREEN,
        youAreController: false,
      });
      await new Promise<void>((resolve) =>
        context.signal.addEventListener("abort", () => resolve(), { once: true }),
      );
    },
    streamTerminalOutput: async function* (
      _req: unknown,
      context: { signal: AbortSignal },
    ) {
      await new Promise<void>((resolve) =>
        context.signal.addEventListener("abort", () => resolve(), { once: true }),
      );
    },
  });
  return createClient(ConnectionService, backend.transport());
}

const aGrpcRuntimeFor = (sessionId: string): SessionRuntimeState => ({
  sessionId,
  status: "connected-grpc",
  attached: true,
  bytesIn: 0,
  bytesOut: 0,
  lastDataReceivedAt: null,
});

const noopInspectorHandlers = {
  onToggleInspector: () => undefined,
  onInspectorClose: () => undefined,
  onInspectorExpand: () => undefined,
  onInspectorRestore: () => undefined,
  onResume: () => undefined,
  onDelete: () => undefined,
  onTerminate: () => undefined,
};

// ===========================================================================
// AC1-AC3 — docked vs drawer layout (full screen)
// ===========================================================================

describe("SessionInspectorDockedDisconnected — docked-vs-drawer layout", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("docks the inspector as the main pane when a disconnected session is selected", () => {
    // Given
    const backend = aConnectionServiceBackend({ sessions: [DISCONNECTED_SESSION] });

    // When
    mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
    page.drawerItem(DISCONNECTED_SESSION.sessionId).click();

    // Then — open AND docked (main pane), not a side drawer
    page.inspectorDrawer().should("have.attr", "data-state", "open");
    page.inspectorDrawer().should("have.attr", "data-docked", "true");
  });

  it("keeps the inspector an overlay drawer when a connected session is selected", () => {
    // Given
    const backend = aConnectionServiceBackend({
      sessions: [CONNECTED_SESSION],
      connectSession: {
        livekitRoom: "room-live",
        livekitUrl: "ws://127.0.0.1:7880",
        livekitServerIdentity: "server",
      },
    });

    // When — connected session auto-closes the inspector, so open it via the toggle
    mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
    page.drawerItem(CONNECTED_SESSION.sessionId).click();
    page.inspectorToggle().click();

    // Then — open but NOT docked (overlay drawer), terminal still present behind it
    page.inspectorDrawer().should("have.attr", "data-state", "open");
    page.inspectorDrawer().should("have.attr", "data-docked", "false");
    page.detailTerminalContainer().should("exist");
  });

  it("keeps the expand and close controls in docked mode; expand keeps it docked", () => {
    // Given
    const backend = aConnectionServiceBackend({ sessions: [DISCONNECTED_SESSION] });

    // When
    mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
    page.drawerItem(DISCONNECTED_SESSION.sessionId).click();
    page.inspectorDrawer().should("have.attr", "data-docked", "true");

    // Then — all controls present in docked mode
    page.inspectorExpand().should("exist");
    page.inspectorClose().should("exist");

    // When — expand
    page.inspectorExpand().click();

    // Then — expanded and still docked
    page.inspectorDrawer().should("have.attr", "data-state", "expanded");
    page.inspectorDrawer().should("have.attr", "data-docked", "true");
  });
});

// ===========================================================================
// AC4-AC5 — "Claim terminal" is suppressed for a disconnected selection only
// ===========================================================================

describe("SessionInspectorDockedDisconnected — Claim terminal suppression", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  it("does NOT render the focused-runtime foreground or Claim terminal for a disconnected session, but keeps the runtime layer mounted", () => {
    // Given — a disconnected session that still has a mounted runtime in the registry
    const client = aClaimDeniedClient();

    // When
    cy.mount(
      <SessionMainPane
        {...noopInspectorHandlers}
        selectedSession={DISCONNECTED_SESSION as unknown as SessionEntry}
        attachment={{ status: "idle" } satisfies SessionAttachmentState}
        inspectorState="open"
        client={client}
        runtimes={[aGrpcRuntimeFor(DISCONNECTED_SESSION.sessionId)]}
        focusedRuntimeId={DISCONNECTED_SESSION.sessionId}
      />,
    );

    // Then — inspector is docked, the focused-runtime marker + Claim overlay are gone, and the
    // runtime layer is still mounted (background sessions keep streaming)
    page.inspectorDrawer().should("have.attr", "data-docked", "true");
    byTestId(TEST_IDS.sessionsRuntimeLayer).should("exist");
    byTestId(TEST_IDS.sessionsDetailTerminalContainer).should("not.exist");
    page.terminalControlOverlay().should("not.exist");
  });

  it("still renders the focused-runtime foreground and Claim terminal for a connected session", () => {
    // Given — a connected session with a mounted focused runtime; another screen holds the lease
    const client = aClaimDeniedClient();

    // When
    cy.mount(
      <SessionMainPane
        {...noopInspectorHandlers}
        selectedSession={CONNECTED_SESSION as unknown as SessionEntry}
        attachment={{ status: "connected-grpc", sessionId: CONNECTED_SESSION.sessionId } satisfies SessionAttachmentState}
        inspectorState="closed"
        client={client}
        runtimes={[aGrpcRuntimeFor(CONNECTED_SESSION.sessionId)]}
        focusedRuntimeId={CONNECTED_SESSION.sessionId}
      />,
    );

    // Then — not docked, focused-runtime marker present, and the Claim terminal CTA shows
    page.inspectorDrawer().should("have.attr", "data-docked", "false");
    byTestId(TEST_IDS.sessionsDetailTerminalContainer).should("exist");
    page.terminalControlOverlay().should("be.visible");
    page.terminalClaimBtn().should("contain.text", "Claim terminal");
  });
});
