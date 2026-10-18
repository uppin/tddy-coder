/**
 * The session Code pane holds one `code_navigation.CodeSession` open with its worktree's code index
 * for as long as it is mounted: opening it warms the index, the index's progress streams in live,
 * and hover / definition / probe queries travel on the same stream — held while the index loads
 * and answered once it is ready. Identifiers that have a definition are underlined on hover, and a
 * plain click on one goes there.
 *
 * Feature: docs/ft/web/1-WIP/PRD-2026-10-18-code-index-warm-on-demand.md (Web, Code pane).
 *
 * All RPC flows through the in-memory backend. The host is `CodeIndexHost`, a fake of the daemon's
 * `CodeSession` handler: it records every frame the pane sends, holds queries while loading, and
 * the test releases them with `finishLoading()`, so held → answered is deterministic.
 */

import "../../src/index.css";
import {
  aCodeIndexHost,
  aCodeLocation,
  anIndexLoading,
  anIndexReady,
  type AskedQuery,
  type CodeIndexHost,
  type IndexPhase,
  type OpenedWorktree,
} from "../support/rpc/codeSessionBackend";
import { aCodePaneOn, THE_WORKTREE } from "../support/drivers/worktreeCodePaneDriver";
import { worktreeCodePanePage } from "../support/pages/worktreeCodePanePage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

// `src/main.rs` calls `geometry::area` on line 4; the call's `area` starts at byte column 26, and
// `main` on line 3 starts at byte column 4.
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
const MAIN = { line: 3, column: 4 };

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

const FETCHING_20: IndexPhase = { title: "Fetching", percentage: 20, message: "crates.io index" };
const INDEXING_38: IndexPhase = { title: "Indexing", percentage: 38, message: "120/310 (tddy_core)" };
const INDEXING_61: IndexPhase = { title: "Indexing", percentage: 61, message: "190/310 (tddy_web)" };

/** A host for the two-file worktree whose index knows `area` and nothing about `main`. */
function aGeometryWorktreeHost(): CodeIndexHost {
  return aCodeIndexHost()
    .withFile("src/main.rs", MAIN_RS)
    .withFile("src/geometry.rs", GEOMETRY_RS)
    .knowing({
      at: { relPath: "src/main.rs", ...THE_CALL_TO_AREA },
      hover: AREA_TYPE,
      definition: [THE_DEFINITION_OF_AREA],
    });
}

// ---------------------------------------------------------------------------
// Host assertions (retried until the host has seen what is expected)
// ---------------------------------------------------------------------------

function expectTheHostToHaveOpened(host: CodeIndexHost, expected: OpenedWorktree[]) {
  cy.wrap(null).should(() => {
    expect(host.openedWorktrees()).to.deep.equal(expected);
  });
}

function expectTheHostToHaveBeenAsked(host: CodeIndexHost, expected: AskedQuery[]) {
  cy.wrap(null).should(() => {
    expect(host.queriesAsked()).to.have.deep.members(expected);
  });
}

function expectTheHostToHaveBeenAskedOfKind(
  host: CodeIndexHost,
  kind: AskedQuery["kind"],
  expected: AskedQuery[],
) {
  cy.wrap(null).should(() => {
    expect(host.queriesAsked().filter((query) => query.kind === kind)).to.deep.equal(expected);
  });
}

function expectOpenAndEndedSessions(host: CodeIndexHost, open: number, ended: number) {
  cy.wrap(null).should(() => {
    expect({ open: host.openSessionCount(), ended: host.endedSessionCount() }).to.deep.equal({
      open,
      ended,
    });
  });
}

const inMainRs = (at: { line: number; column: number }) => ({ relPath: "src/main.rs", ...at });

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 900);
});

// ---------------------------------------------------------------------------
// The session and its progress
// ---------------------------------------------------------------------------

it("opening the pane opens a code session for its worktree and shows live progress until ready", () => {
  // Given a worktree whose index is fetching its dependencies
  const host = aGeometryWorktreeHost().startingAt(anIndexLoading(FETCHING_20));

  // When the pane opens
  aCodePaneOn(host).mount();

  // Then it opened one code session naming its worktree
  expectTheHostToHaveOpened(host, [THE_WORKTREE]);
  // And the strip shows the phase running now with its message
  worktreeCodePanePage.indexStatus().should("have.attr", "data-state", "progress");
  worktreeCodePanePage.indexStatusSummary().should("have.text", "Indexing — Fetching 20%");
  worktreeCodePanePage.indexStatusDetail().should("have.text", "crates.io index");

  // When the index moves on to indexing
  cy.then(() => host.reportProgress(anIndexLoading(INDEXING_38)));

  // Then the strip follows it
  worktreeCodePanePage.indexStatusSummary().should("have.text", "Indexing — Indexing 38%");
  worktreeCodePanePage.indexStatusDetail().should("have.text", "120/310 (tddy_core)");

  // When the index finishes loading
  cy.then(() => host.finishLoading());

  // Then the strip says it is ready
  worktreeCodePanePage.indexStatus().should("have.attr", "data-state", "ready");
  worktreeCodePanePage.indexStatusSummary().should("have.text", "Code index ready");
  worktreeCodePanePage.indexStatusDetail().should("not.exist");
});

it("shows every phase the index is running at once", () => {
  // Given an index fetching and indexing at the same time — fetching began first, indexing reported
  // most recently
  const host = aGeometryWorktreeHost().startingAt(anIndexLoading(FETCHING_20, INDEXING_38));

  // When the pane opens
  aCodePaneOn(host).mount();

  // Then the strip leads with the phase reported on most recently and counts the other
  worktreeCodePanePage.indexStatusSummary().should("have.text", "Indexing — Indexing 38%");
  worktreeCodePanePage.indexStatusDetail().should("have.text", "120/310 (tddy_core)");
  worktreeCodePanePage.indexStatusMore().should("have.text", "+1 more");
  // And its tooltip lists every phase with its percentage and message
  worktreeCodePanePage
    .indexStatusMore()
    .should("have.attr", "title")
    .and("include", "Fetching 20%")
    .and("include", "crates.io index")
    .and("include", "Indexing 38%")
    .and("include", "120/310 (tddy_core)");
});

// ---------------------------------------------------------------------------
// Held queries
// ---------------------------------------------------------------------------

it("a hover while the index loads shows it loading and fills in when answered", () => {
  // Given main.rs previewed while the index is indexing
  const host = aGeometryWorktreeHost().startingAt(anIndexLoading(INDEXING_38));
  const pane = aCodePaneOn(host).mount().previewing("src/main.rs");

  // When the pointer rests on the call to `area`
  pane.dwellOn(THE_CALL_TO_AREA);

  // Then the pane probed it and asked for its hover
  expectTheHostToHaveBeenAsked(host, [
    { kind: "PROBE", ...inMainRs(THE_CALL_TO_AREA) },
    { kind: "HOVER", ...inMainRs(THE_CALL_TO_AREA) },
  ]);
  // And the card opens saying the index is loading
  worktreeCodePanePage.hoverCard().should("be.visible");
  worktreeCodePanePage.hoverLoading().should("have.text", "Index loading — Indexing 38%");

  // When the index reports further progress
  cy.then(() => host.reportProgress(anIndexLoading(INDEXING_61)));

  // Then the loading card follows it
  worktreeCodePanePage.hoverLoading().should("have.text", "Index loading — Indexing 61%");

  // When the index finishes loading and answers the held hover
  cy.then(() => host.finishLoading());

  // Then the card fills in with the type
  worktreeCodePanePage.hoverLoading().should("not.exist");
  worktreeCodePanePage.hoverBody().should("be.visible").and("contain.text", AREA_TYPE);
});

it("go to definition while the index loads never says no definition found", () => {
  // Given main.rs previewed while the index is indexing
  const host = aGeometryWorktreeHost().startingAt(anIndexLoading(INDEXING_38));
  const pane = aCodePaneOn(host).mount().previewing("src/main.rs");

  // When going to the definition of `area`
  pane.goToDefinitionOf(THE_CALL_TO_AREA);

  // Then the pane asked for it once
  expectTheHostToHaveBeenAskedOfKind(host, "DEFINITION", [
    { kind: "DEFINITION", ...inMainRs(THE_CALL_TO_AREA) },
  ]);
  // And the notice says the index is loading rather than that nothing was found
  worktreeCodePanePage.notice().should("have.text", "Index loading — Indexing 38%");

  // When the index finishes loading and answers the held definition
  cy.then(() => host.finishLoading());

  // Then the definition opens, scrolled to its line, and the notice is gone
  worktreeCodePanePage
    .line(DEFINITION_LINE)
    .should("be.visible")
    .and("have.attr", "data-navigation-target", "true")
    .and("contain.text", "pub fn area(width: u32, height: u32) -> u32 {");
  worktreeCodePanePage.notice().should("not.exist");
});

it("says no definition found only for an answer from a loaded index", () => {
  // Given main.rs previewed with the index loaded, and nothing defines `main` for it
  const host = aGeometryWorktreeHost().startingAt(anIndexReady());
  const pane = aCodePaneOn(host).mount().previewing("src/main.rs");

  // When going to the definition of `main`
  pane.goToDefinitionOf(MAIN);

  // Then the loaded index's empty answer reads as no definition
  expectTheHostToHaveBeenAskedOfKind(host, "DEFINITION", [
    { kind: "DEFINITION", ...inMainRs(MAIN) },
  ]);
  worktreeCodePanePage.notice().should("have.text", "No definition found");
});

// ---------------------------------------------------------------------------
// Probe: underline and plain click
// ---------------------------------------------------------------------------

it("underlines an identifier that has a definition and leaves one without plain", () => {
  // Given main.rs previewed with the index loaded
  const host = aGeometryWorktreeHost().startingAt(anIndexReady());
  const pane = aCodePaneOn(host).mount().previewing("src/main.rs");

  // When the pointer rests on `main`, which has no definition, and then on the call to `area`
  pane.dwellOn(MAIN);
  expectTheHostToHaveBeenAskedOfKind(host, "PROBE", [{ kind: "PROBE", ...inMainRs(MAIN) }]);
  pane.dwellOn(THE_CALL_TO_AREA);

  // Then `area` is underlined with a pointer cursor
  worktreeCodePanePage
    .identifier(THE_CALL_TO_AREA.line, THE_CALL_TO_AREA.column)
    .should("have.attr", "data-definable", "true")
    .and("have.css", "text-decoration-line", "underline")
    .and("have.css", "cursor", "pointer");
  // And `main` stays plain
  worktreeCodePanePage
    .identifier(MAIN.line, MAIN.column)
    .should("not.have.attr", "data-definable", "true")
    .and("have.css", "text-decoration-line", "none");
});

it("a plain click on an underlined identifier opens its definition", () => {
  // Given main.rs previewed with the index loaded, and the call to `area` underlined after a dwell
  const host = aGeometryWorktreeHost().startingAt(anIndexReady());
  const pane = aCodePaneOn(host).mount().previewing("src/main.rs");
  pane.dwellOn(THE_CALL_TO_AREA);
  worktreeCodePanePage
    .identifier(THE_CALL_TO_AREA.line, THE_CALL_TO_AREA.column)
    .should("have.attr", "data-definable", "true");

  // When clicking it with no modifier key
  pane.plainClick(THE_CALL_TO_AREA);

  // Then the definition opens, scrolled to its line
  worktreeCodePanePage
    .line(DEFINITION_LINE)
    .should("be.visible")
    .and("have.attr", "data-navigation-target", "true")
    .and("contain.text", "pub fn area(width: u32, height: u32) -> u32 {");
  // And it went where the probe pointed, without asking for the definition again
  expectTheHostToHaveBeenAskedOfKind(host, "DEFINITION", []);
});

// ---------------------------------------------------------------------------
// Closing
// ---------------------------------------------------------------------------

it("closing the pane ends its code session", () => {
  // Given an open pane holding its code session
  const host = aGeometryWorktreeHost().startingAt(anIndexReady());
  const pane = aCodePaneOn(host).mount();
  expectOpenAndEndedSessions(host, 1, 0);

  // When the pane closes
  pane.close();

  // Then its code session has ended
  expectOpenAndEndedSessions(host, 0, 1);
});
