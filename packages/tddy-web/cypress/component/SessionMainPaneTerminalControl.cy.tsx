/**
 * Acceptance tests: when the session pane puts a terminal-control overlay on screen.
 *
 * PRD: docs/ft/daemon/terminal-sessions.md (control section) and
 *      docs/ft/web/session-drawer.md (Claim terminal CTA section).
 *
 * A session can be selected and attached before any runtime has registered itself, and the pane
 * shows its transition placeholder for that gap. Nothing holds a control lease yet, so there is
 * nothing to claim — the overlay must stay away until a runtime is there. What the overlay itself
 * renders once the pane does mount one is `TerminalControlAcceptance.cy.tsx`'s subject.
 */

import React from "react";
import { SessionMainPane } from "../../src/components/sessions/SessionMainPane";
import type { SessionAttachmentState } from "../../src/components/sessions/useSessionAttachment";
import { aSessionConnection } from "../support/rpc/sessionConnections";
import type { SessionEntry } from "../../src/gen/connection_pb";
import { sessionsDrawerPage as page } from "../support/pages/sessionsDrawerPage";

// ---------------------------------------------------------------------------
// Shared fixtures
// ---------------------------------------------------------------------------

const SESSION_ID = "control-test-session-1";

const aSelectedSession: Partial<SessionEntry> = {
  sessionId: SESSION_ID,
  isActive: true,
  status: "active",
  repoPath: "/home/user/my-project",
};

/** A session its host serves itself — plain RPC, no room. */
const anAttachedSession: SessionAttachmentState = {
  status: "connected",
  connection: aSessionConnection(SESSION_ID).build(),
};

const noopHandlers = {
  inspectorState: "closed" as const,
  onToggleInspector: () => undefined,
  onInspectorClose: () => undefined,
  onInspectorExpand: () => undefined,
  onInspectorRestore: () => undefined,
  onResume: () => undefined,
  onDelete: () => undefined,
  onTerminate: () => undefined,
};

// ---------------------------------------------------------------------------
// When no runtime is attached yet (session selected but not connected), no overlay renders.
// ---------------------------------------------------------------------------

it("does not render the Claim terminal overlay when no runtime is attached yet", () => {
  // Given — session selected and connected, but the runtime layer has not registered a runtime yet
  cy.mount(
    <SessionMainPane
      {...noopHandlers}
      selectedSession={aSelectedSession as SessionEntry}
      attachment={anAttachedSession}
      runtimes={[]}
      focusedRuntimeId={null}
    />,
  );

  // Then — the transition placeholder container exists, but no control overlay is rendered
  page.detailTerminalContainer().should("exist");
  page.terminalControlOverlay().should("not.exist");
});
