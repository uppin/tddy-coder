/**
 * Acceptance tests: code navigation in the session Code pane — ctrl/cmd-click goes to a definition,
 * hover shows an identifier's type, and the references list navigates to a reference.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-10-03-code-navigation.md.
 *
 * All RPC calls flow through the in-memory backend — no HTTP intercepts. Navigation is answered by
 * `code_navigation.CodeNavigationService`; positions on the wire are one-based lines and one-based
 * byte columns.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import {
  CodeLocationSchema,
  CodeNavigationService,
  DefinitionResponseSchema,
  HoverResponseSchema,
  ReferencesResponseSchema,
  SourcePositionSchema,
  SourceRangeSchema,
} from "../../src/gen/code_navigation_pb";
import {
  ListWorktreeDirectoryResponseSchema,
  ReadWorktreeFileResponseSchema,
  WorktreeDirEntrySchema,
  WorktreeService,
} from "../../src/gen/worktree_pb";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import { requestFields } from "../support/rpc/recordedRequests";
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

function aLocation(relPath: string, line: number, startColumn: number, endColumn: number) {
  return create(CodeLocationSchema, {
    relPath,
    range: create(SourceRangeSchema, {
      start: create(SourcePositionSchema, { line, column: startColumn }),
      end: create(SourcePositionSchema, { line, column: endColumn }),
    }),
    outsideWorktree: false,
  });
}

const THE_DEFINITION_OF_AREA = aLocation("src/geometry.rs", DEFINITION_LINE, 8, 12);
const THE_CALL_IN_MAIN = aLocation("src/main.rs", 4, 26, 30);

// A worktree holding `src/main.rs` and `src/geometry.rs`, whose navigation service knows where
// `area` is defined, what it is, and where it is referenced.
function aNavigableWorktreeBackend() {
  const directories: Record<string, Array<{ name: string; isDir: boolean }>> = {
    "": [{ name: "src", isDir: true }],
    src: [
      { name: "geometry.rs", isDir: false },
      { name: "main.rs", isDir: false },
    ],
  };
  const files: Record<string, string> = {
    "src/main.rs": MAIN_RS,
    "src/geometry.rs": GEOMETRY_RS,
  };

  return aSessionsDrawerBackend([SESSION])
    .onUnary(WorktreeService.method.listWorktreeDirectory, (req) =>
      create(ListWorktreeDirectoryResponseSchema, {
        entries: (directories[req.relPath] ?? []).map((e) => create(WorktreeDirEntrySchema, e)),
      }),
    )
    .onUnary(WorktreeService.method.readWorktreeFile, (req) =>
      create(ReadWorktreeFileResponseSchema, {
        contentUtf8: files[req.relPath] ?? "",
        truncated: false,
        byteSize: BigInt((files[req.relPath] ?? "").length),
      }),
    )
    .onUnary(CodeNavigationService.method.definition, () =>
      create(DefinitionResponseSchema, { locations: [THE_DEFINITION_OF_AREA] }),
    )
    .onUnary(CodeNavigationService.method.hover, () =>
      create(HoverResponseSchema, { markdown: AREA_TYPE }),
    )
    .onUnary(CodeNavigationService.method.references, () =>
      create(ReferencesResponseSchema, { locations: [THE_DEFINITION_OF_AREA, THE_CALL_IN_MAIN] }),
    );
}

/** Mount the drawer, open the session's Code pane and preview `src/main.rs`. */
function previewMainRsIn(backend: ReturnType<typeof aNavigableWorktreeBackend>) {
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
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
  window.localStorage.setItem("tddy_session_token", SESSION_TOKEN);
});

// ---------------------------------------------------------------------------
// Go to definition
// ---------------------------------------------------------------------------

it("ctrl-click on an identifier opens the definition scrolled to its line", () => {
  // Given
  const backend = aNavigableWorktreeBackend();
  previewMainRsIn(backend);

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
  // And the daemon was asked about the clicked position in the session's worktree.
  cy.wrap(null).should(() => {
    const calls = backend.callsTo(CodeNavigationService.method.definition);
    expect(calls).to.have.length(1);
    const { position, ...request } = calls[0];
    expect(requestFields(request)).to.deep.equal({
      sessionToken: SESSION_TOKEN,
      projectId: PROJECT_ID,
      worktreePath: WORKTREE_PATH,
      relPath: "src/main.rs",
    });
    expect(requestFields(position!)).to.deep.equal(THE_CALL_TO_AREA);
  });
});

// ---------------------------------------------------------------------------
// Hover
// ---------------------------------------------------------------------------

it("hovering an identifier shows its type", () => {
  // Given
  const backend = aNavigableWorktreeBackend();
  previewMainRsIn(backend);

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
  const backend = aNavigableWorktreeBackend();
  previewMainRsIn(backend);
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
