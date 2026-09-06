/**
 * Acceptance tests: the Session Inspector is an **overlay drawer for every session** — it no longer
 * docks as the main pane for a disconnected one. Docking existed only because a dormant session's
 * pane was empty behind it; it now shows that session's recorded activities, which a full-pane
 * drawer would bury.
 *
 * What survives the change: the "Claim terminal" overlay is still not rendered for an inactive
 * session (its focused runtime foreground stays suppressed — now keyed on the session's inactivity
 * rather than on the inspector being docked) while the runtime layer stays mounted so background
 * sessions keep streaming.
 *
 * PRD: docs/ft/web/inactive-session-activities.md § Inspector
 *      docs/ft/web/session-drawer.md § Session Inspector Drawer
 *
 * These assert the drawer's *state* and what survives beside it, not its geometry: the component
 * harness (`cypress/support/component-index.html`) loads no stylesheet, so every Tailwind class is
 * inert and every element measures full width. Width or offset assertions would pass or fail for
 * reasons unrelated to the layout they claim to check.
 *
 * Layout ACs drive the full SessionsDrawerScreen over the recording LiveKit harness (mirrors
 * SessionInspectorAcceptance). The Claim-terminal ACs drive SessionMainPane directly with an
 * explicit runtime + a fake ConnectionService client (mirrors TerminalControlAcceptance /
 * SessionRuntimeStealClaimReattach), since they need a runtime that is still mounted for a session
 * that is / is not disconnected.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { createClient, type Transport } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  ConnectionService,
  ClaimTerminalControlResponseSchema,
  TerminalControlEventSchema,
} from "../../src/gen/connection_pb";
import type { SessionEntry } from "../../src/gen/connection_pb";
import type { SessionRuntimeState } from "../../src/components/sessions/sessionRuntimeRegistry";
import type { SessionAttachmentState } from "../../src/components/sessions/useSessionAttachment";
import { aSessionConnection } from "../support/rpc/sessionConnections";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { SessionMainPane } from "../../src/components/sessions/SessionMainPane";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aConnectionServiceBackend } from "../support/rpc/connectionServiceBackend";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { sessionsDrawerPage as page } from "../support/pages/sessionsDrawerPage";
import { sessionActivitiesPage } from "../support/pages/sessionActivitiesPage";
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
// Fake ConnectionService transport for the SessionMainPane-direct runtime tests
// ---------------------------------------------------------------------------

const OTHER_SCREEN = "screen-held-by-another-9999";

/** A ConnectionService whose control lease is held by another screen (so the focused runtime's
 *  auto-claim is denied and the "Claim terminal" CTA WOULD show), with a never-ending terminal
 *  output stream so the runtime mounts and stays stable. */
function aClaimDeniedTransport() {
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
  return backend.transport();
}

/** A runtime whose session is served by its host itself — plain RPC over `transport`. */
const aHostServedRuntimeFor = (sessionId: string, transport: Transport): SessionRuntimeState => ({
  sessionId,
  connection: aSessionConnection(sessionId).servingOver(transport).build(),
  hint: { sessionId },
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
// Overlay-drawer layout, whatever the session's liveness (full screen)
// ===========================================================================

describe("SessionInactiveInspectorOverlay — overlay-drawer layout", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("keeps a disconnected session's activities mounted behind the open inspector", () => {
    // Given
    const backend = aConnectionServiceBackend({
      sessions: [DISCONNECTED_SESSION],
      acpReplay: { counts: [0], snapshot: [] },
    });

    // When — the inspector no longer opens on its own, so ask for it
    mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
    page.drawerItem(DISCONNECTED_SESSION.sessionId).click();
    page.inspectorToggle().click();

    // Then — the drawer opens over a pane that still owns its base view. The docked layout used to
    // *replace* that pane, so its survival is what the removal of docking buys.
    page.inspectorDrawer().should("have.attr", "data-state", "open");
    sessionActivitiesPage.pane().should("exist");
  });

  it("keeps a connected session's terminal mounted behind the open inspector", () => {
    // Given
    const backend = aConnectionServiceBackend({
      sessions: [CONNECTED_SESSION],
      connectSession: {
        livekitRoom: "room-live",
        livekitUrl: "ws://127.0.0.1:7880",
        livekitServerIdentity: "server",
      },
    });

    // When
    mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
    page.drawerItem(CONNECTED_SESSION.sessionId).click();
    page.inspectorToggle().click();

    // Then — unchanged from before this feature: an active session's drawer was always an overlay
    page.inspectorDrawer().should("have.attr", "data-state", "open");
    page.detailTerminalContainer().should("exist");
  });

  it("expands a disconnected session's inspector on request", () => {
    // Given
    const backend = aConnectionServiceBackend({
      sessions: [DISCONNECTED_SESSION],
      acpReplay: { counts: [0], snapshot: [] },
    });

    // When
    mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
    page.drawerItem(DISCONNECTED_SESSION.sessionId).click();
    page.inspectorToggle().click();
    page.inspectorDrawer().should("have.attr", "data-state", "open");
    page.inspectorExpand().click();

    // Then
    page.inspectorDrawer().should("have.attr", "data-state", "expanded");
  });

  it("keeps the expand and close controls available for a disconnected session", () => {
    // Given
    const backend = aConnectionServiceBackend({
      sessions: [DISCONNECTED_SESSION],
      acpReplay: { counts: [0], snapshot: [] },
    });

    // When
    mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
    page.drawerItem(DISCONNECTED_SESSION.sessionId).click();
    page.inspectorToggle().click();

    // Then
    page.inspectorExpand().should("exist");
    page.inspectorClose().should("exist");
  });
});

// ===========================================================================
// "Claim terminal" is suppressed for an inactive selection only
// ===========================================================================

describe("SessionInactiveInspectorOverlay — Claim terminal suppression", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  it("does NOT render the focused-runtime foreground or Claim terminal for a disconnected session, but keeps the runtime layer mounted", () => {
    // Given — a disconnected session that still has a mounted runtime in the registry
    const transport = aClaimDeniedTransport();

    // When
    cy.mount(
      <SessionMainPane
        // No host connection in scope: this spec is not about the inspector's media tabs,
        // and `host` is required so that saying so is a choice rather than an omission.
        host={null}
        {...noopInspectorHandlers}
        selectedSession={DISCONNECTED_SESSION as unknown as SessionEntry}
        attachment={{ status: "idle" } satisfies SessionAttachmentState}
        inspectorState="open"
        client={createClient(ConnectionService, transport)}
        runtimes={[aHostServedRuntimeFor(DISCONNECTED_SESSION.sessionId, transport)]}
        focusedRuntimeId={DISCONNECTED_SESSION.sessionId}
      />,
    );

    // Then — the focused-runtime marker + Claim overlay are gone, and the runtime layer is still
    // mounted (background sessions keep streaming)
    byTestId(TEST_IDS.sessionsRuntimeLayer).should("exist");
    byTestId(TEST_IDS.sessionsDetailTerminalContainer).should("not.exist");
    page.terminalControlOverlay().should("not.exist");
  });

  it("still renders the focused-runtime foreground and Claim terminal for a connected session", () => {
    // Given — a connected session with a mounted focused runtime; another screen holds the lease
    const transport = aClaimDeniedTransport();

    // When
    cy.mount(
      <SessionMainPane
        host={null}
        {...noopInspectorHandlers}
        selectedSession={CONNECTED_SESSION as unknown as SessionEntry}
        attachment={
          {
            status: "connected",
            connection: aSessionConnection(CONNECTED_SESSION.sessionId)
              .servingOver(transport)
              .build(),
          } satisfies SessionAttachmentState
        }
        inspectorState="closed"
        client={createClient(ConnectionService, transport)}
        runtimes={[aHostServedRuntimeFor(CONNECTED_SESSION.sessionId, transport)]}
        focusedRuntimeId={CONNECTED_SESSION.sessionId}
      />,
    );

    // Then — focused-runtime marker present, and the Claim terminal CTA shows
    byTestId(TEST_IDS.sessionsDetailTerminalContainer).should("exist");
    page.terminalControlOverlay().should("be.visible");
    page.terminalClaimBtn().should("contain.text", "Claim terminal");
  });
});
