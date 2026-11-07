/**
 * Acceptance tests: code navigation in the session Code pane — ctrl/cmd-click goes to a definition,
 * hover shows an identifier's type, and the references list navigates to a reference.
 *
 * Feature: docs/ft/web/session-code-pane.md (Code navigation).
 *
 * All RPC calls flow through the in-memory backend — no HTTP intercepts. Navigation travels on the
 * pane's `code_navigation.CodeSession`, answered by `CodeIndexHost` with the index loaded;
 * positions on the wire are one-based lines and one-based byte columns. What a loading index does
 * to these queries is `WorktreeCodePaneCodeSession.cy.tsx`'s.
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import { anIndexReady, type CodeIndexHost } from "../support/rpc/codeSessionBackend";
import { expectTheHost } from "../support/rpc/codeSessionAssertions";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { codePreview } from "../support/drivers/worktreeCodePaneDriver";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { worktreeCodePanePage } from "../support/pages/worktreeCodePanePage";
import {
  AREA_TYPE,
  DEFINITION_LINE,
  THE_CALL_TO_AREA,
  THE_LINE_DEFINING_AREA,
  aGeometryWorktreeHost,
  inMainRs,
} from "../support/fixtures/geometryWorktree";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const SESSION_TOKEN = "fake-token";
const PROJECT_ID = "proj-code-navigation";
const WORKTREE_PATH = "/home/dev/code-navigation-project";

const SESSION = {
  sessionId: "claude-cli-session-0000-0000-0000-00000000c0de",
  createdAt: "2026-10-03T09:00:00Z",
  status: "idle",
  repoPath: WORKTREE_PATH,
  pid: 0,
  isActive: false,
  projectId: PROJECT_ID,
  daemonInstanceId: "",
  workflowGoal: "",
  pendingElicitation: false,
  orchestratorSessionId: "",
  recipe: "",
  sessionType: "claude-cli",
};

/**
 * The geometry worktree, whose loaded index knows where `area` is defined, what it is, and where it
 * is referenced.
 */
function aNavigableWorktreeHost(): CodeIndexHost {
  return aGeometryWorktreeHost().startingAt(anIndexReady());
}

/** The file holding the definition of `area` is open, scrolled to its line and marked as the target. */
function expectTheDefinitionOfAreaOpen() {
  worktreeCodePanePage
    .line(DEFINITION_LINE)
    .should("be.visible")
    .and("have.attr", "data-navigation-target", "true")
    .and("contain.text", THE_LINE_DEFINING_AREA);
}

/** Mount the drawer, open the session's Code pane and preview `src/main.rs`. */
function previewMainRsIn(host: CodeIndexHost) {
  mountWithRpc(
    withSelectedDaemon(<SessionsDrawerScreen />),
    host.backend(aSessionsDrawerBackend([SESSION])),
  );
  sessionsDrawerPage.drawerItem(SESSION.sessionId).click();
  worktreeCodePanePage.toggle().click();
  worktreeCodePanePage.node("src").click();
  worktreeCodePanePage.node("src/main.rs").click();
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800); // desktop: session list defaults open so drawer items are clickable
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  cy.then(() => window.localStorage.setItem("tddy_session_token", SESSION_TOKEN));
});

// ---------------------------------------------------------------------------
// Go to definition
// ---------------------------------------------------------------------------

it("ctrl-click on an identifier opens the definition scrolled to its line", () => {
  // Given
  const host = aNavigableWorktreeHost();
  previewMainRsIn(host);

  // When
  codePreview.goToDefinitionOf(THE_CALL_TO_AREA);

  // Then — the definition file is open, its line scrolled into view and marked as the target.
  expectTheDefinitionOfAreaOpen();
  // And the pane's code session is on the session's worktree and was asked about the clicked position.
  expectTheHost(host)
    .toHaveOpened([{ sessionToken: SESSION_TOKEN, projectId: PROJECT_ID, worktreePath: WORKTREE_PATH }])
    .toHaveBeenAskedOfKind("DEFINITION", [{ kind: "DEFINITION", ...inMainRs(THE_CALL_TO_AREA) }]);
});

// ---------------------------------------------------------------------------
// Hover
// ---------------------------------------------------------------------------

it("hovering an identifier shows its type", () => {
  // Given
  previewMainRsIn(aNavigableWorktreeHost());

  // When
  codePreview.dwellOn(THE_CALL_TO_AREA);

  // Then
  worktreeCodePanePage.hoverCard().should("be.visible");
  worktreeCodePanePage.hoverBody().should("have.text", AREA_TYPE);
});

// ---------------------------------------------------------------------------
// References
// ---------------------------------------------------------------------------

it("the references list navigates to a reference", () => {
  // Given — the references of `area` are listed from its hover card.
  previewMainRsIn(aNavigableWorktreeHost());
  codePreview.dwellOn(THE_CALL_TO_AREA);
  codePreview.listReferences();
  worktreeCodePanePage.reference("src/main.rs", 4).should("be.visible");

  // When
  worktreeCodePanePage.reference("src/geometry.rs", DEFINITION_LINE).click();

  // Then — the referenced file is open at the referenced line.
  expectTheDefinitionOfAreaOpen();
});
