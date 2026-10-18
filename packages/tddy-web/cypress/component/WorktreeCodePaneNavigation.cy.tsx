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
import {
  aCodeIndexHost,
  aCodeLocation,
  anIndexReady,
  type CodeIndexHost,
} from "../support/rpc/codeSessionBackend";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { worktreeCodePanePage } from "../support/pages/worktreeCodePanePage";

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

// `src/main.rs` calls `geometry::area` on line 4; the call's `area` starts at byte column 26.
const MAIN_RS = [
  "mod geometry;",
  "",
  "fn main() {",
  "    let area = geometry::area(3, 4);",
  '    println!("{area}");',
  "}",
  "",
].join("\n");
const THE_CALL_TO_AREA = { line: 4, column: 26 };

// `src/geometry.rs` defines `area` on line 120 — far enough down that opening the file at its top
// would leave the definition out of view, so "scrolled to its line" is observable.
const DEFINITION_LINE = 120;
const GEOMETRY_RS = [
  ...Array.from({ length: DEFINITION_LINE - 1 }, (_, i) => `// geometry note ${i + 1}`),
  "pub fn area(width: u32, height: u32) -> u32 {",
  "    width * height",
  "}",
  "",
].join("\n");

const AREA_TYPE = "fn area(width: u32, height: u32) -> u32";

const THE_DEFINITION_OF_AREA = aCodeLocation("src/geometry.rs", DEFINITION_LINE, 8, 12);
const THE_CALL_IN_MAIN = aCodeLocation("src/main.rs", 4, 26, 30);

// A worktree holding `src/main.rs` and `src/geometry.rs`, whose loaded index knows where `area` is
// defined, what it is, and where it is referenced.
function aNavigableWorktreeHost(): CodeIndexHost {
  return aCodeIndexHost()
    .withFile("src/main.rs", MAIN_RS)
    .withFile("src/geometry.rs", GEOMETRY_RS)
    .startingAt(anIndexReady())
    .knowing({
      at: { relPath: "src/main.rs", ...THE_CALL_TO_AREA },
      hover: AREA_TYPE,
      definition: [THE_DEFINITION_OF_AREA],
      references: [THE_DEFINITION_OF_AREA, THE_CALL_IN_MAIN],
    });
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
  worktreeCodePanePage
    .identifier(THE_CALL_TO_AREA.line, THE_CALL_TO_AREA.column)
    .click({ ctrlKey: true });

  // Then — the definition file is open, its line scrolled into view and marked as the target.
  worktreeCodePanePage
    .line(DEFINITION_LINE)
    .should("be.visible")
    .and("have.attr", "data-navigation-target", "true")
    .and("contain.text", "pub fn area(width: u32, height: u32) -> u32 {");
  // And the pane's code session is on the session's worktree and was asked about the clicked position.
  cy.wrap(null).should(() => {
    expect(host.openedWorktrees()).to.deep.equal([
      { sessionToken: SESSION_TOKEN, projectId: PROJECT_ID, worktreePath: WORKTREE_PATH },
    ]);
    expect(host.queriesAsked().filter((query) => query.kind === "DEFINITION")).to.deep.equal([
      { kind: "DEFINITION", relPath: "src/main.rs", ...THE_CALL_TO_AREA },
    ]);
  });
});

// ---------------------------------------------------------------------------
// Hover
// ---------------------------------------------------------------------------

it("hovering an identifier shows its type", () => {
  // Given
  previewMainRsIn(aNavigableWorktreeHost());

  // When
  worktreeCodePanePage
    .identifier(THE_CALL_TO_AREA.line, THE_CALL_TO_AREA.column)
    .trigger("mouseover");

  // Then
  worktreeCodePanePage.hoverCard().should("be.visible").and("contain.text", AREA_TYPE);
});

// ---------------------------------------------------------------------------
// References
// ---------------------------------------------------------------------------

it("the references list navigates to a reference", () => {
  // Given — the references of `area` are listed from its hover card.
  previewMainRsIn(aNavigableWorktreeHost());
  worktreeCodePanePage
    .identifier(THE_CALL_TO_AREA.line, THE_CALL_TO_AREA.column)
    .trigger("mouseover");
  worktreeCodePanePage.referencesAction().click();
  worktreeCodePanePage.reference("src/main.rs", 4).should("be.visible");

  // When
  worktreeCodePanePage.reference("src/geometry.rs", DEFINITION_LINE).click();

  // Then — the referenced file is open at the referenced line.
  worktreeCodePanePage
    .line(DEFINITION_LINE)
    .should("be.visible")
    .and("have.attr", "data-navigation-target", "true")
    .and("contain.text", "pub fn area(width: u32, height: u32) -> u32 {");
});
