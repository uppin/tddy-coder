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
 * the test releases them with `finishLoading()` / `failLoading()`, so held → answered is
 * deterministic. What a dropped session does is `WorktreeCodePaneReconnect.cy.tsx`'s.
 */

import "../../src/index.css";
import { anIndexLoading, anIndexReady } from "../support/rpc/codeSessionBackend";
import { expectTheHost } from "../support/rpc/codeSessionAssertions";
import { aCodePaneOn, THE_WORKTREE } from "../support/drivers/worktreeCodePaneDriver";
import { worktreeCodePanePage } from "../support/pages/worktreeCodePanePage";
import {
  AREA_TYPE,
  DEFINITION_LINE,
  FETCHING_20,
  INDEXING_38,
  INDEXING_61,
  MAIN,
  THE_CALL_TO_AREA,
  THE_LINE_DEFINING_AREA,
  aGeometryWorktreeHost,
  inMainRs,
} from "../support/fixtures/geometryWorktree";

/** The definition of `area` is open in the preview, scrolled to its line and marked as the target. */
function expectTheDefinitionOfAreaOpen() {
  worktreeCodePanePage
    .line(DEFINITION_LINE)
    .should("be.visible")
    .and("have.attr", "data-navigation-target", "true")
    .and("contain.text", THE_LINE_DEFINING_AREA);
}

/** The identifier at `at` is underlined, with a pointer cursor: a plain click goes to its definition. */
function expectUnderlined(at: { line: number; column: number }) {
  worktreeCodePanePage
    .identifier(at.line, at.column)
    .should("have.attr", "data-definable", "true")
    .and("have.css", "text-decoration-line", "underline")
    .and("have.css", "cursor", "pointer");
}

/** The identifier at `at` is plain text: not marked definable, not underlined. */
function expectPlain(at: { line: number; column: number }) {
  worktreeCodePanePage.identifier(at.line, at.column).should(($identifier) => {
    expect($identifier).not.to.have.attr("data-definable");
    expect($identifier).to.have.css("text-decoration-line", "none");
  });
}

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
  expectTheHost(host).toHaveOpened([THE_WORKTREE]);
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
  // And its tooltip lists every phase, in the order they began, with its percentage and message
  worktreeCodePanePage
    .indexStatusMore()
    .should(
      "have.attr",
      "title",
      "Fetching 20% — crates.io index\nIndexing 38% — 120/310 (tddy_core)",
    );
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
  expectTheHost(host).toHaveBeenAsked([
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
  worktreeCodePanePage.hoverBody().should("be.visible").and("have.text", AREA_TYPE);
});

it("closing a loading hover card cancels its query on the host", () => {
  // Given a hover on the call to `area` held by the loading index
  const host = aGeometryWorktreeHost().startingAt(anIndexLoading(INDEXING_38));
  const pane = aCodePaneOn(host).mount().previewing("src/main.rs");
  pane.dwellOn(THE_CALL_TO_AREA);
  worktreeCodePanePage.hoverLoading().should("be.visible");

  // When the card is closed
  pane.closeHoverCard();

  // Then the pane cancelled the held hover, and only it
  expectTheHost(host).toHaveCancelled([{ kind: "HOVER", ...inMainRs(THE_CALL_TO_AREA) }]);
  worktreeCodePanePage.hoverCard().should("not.exist");
});

it("moving to another identifier cancels the held hover of the one before", () => {
  // Given a hover on the call to `area` held by the loading index
  const host = aGeometryWorktreeHost().startingAt(anIndexLoading(INDEXING_38));
  const pane = aCodePaneOn(host).mount().previewing("src/main.rs");
  pane.dwellOn(THE_CALL_TO_AREA);
  worktreeCodePanePage.hoverLoading().should("be.visible");

  // When the pointer rests on `main` instead
  pane.dwellOn(MAIN);

  // Then the pane asked for `main`'s hover and cancelled `area`'s
  expectTheHost(host).toHaveBeenAskedOfKind("HOVER", [
    { kind: "HOVER", ...inMainRs(THE_CALL_TO_AREA) },
    { kind: "HOVER", ...inMainRs(MAIN) },
  ]);
  expectTheHost(host).toHaveCancelled([{ kind: "HOVER", ...inMainRs(THE_CALL_TO_AREA) }]);
});

it("go to definition while the index loads never says no definition found", () => {
  // Given main.rs previewed while the index is indexing
  const host = aGeometryWorktreeHost().startingAt(anIndexLoading(INDEXING_38));
  const pane = aCodePaneOn(host).mount().previewing("src/main.rs");

  // When going to the definition of `area`
  pane.goToDefinitionOf(THE_CALL_TO_AREA);

  // Then the pane asked for it once
  expectTheHost(host).toHaveBeenAskedOfKind("DEFINITION", [
    { kind: "DEFINITION", ...inMainRs(THE_CALL_TO_AREA) },
  ]);
  // And the notice says the index is loading rather than that nothing was found
  worktreeCodePanePage.notice().should("have.text", "Index loading — Indexing 38%");

  // When the index finishes loading and answers the held definition
  cy.then(() => host.finishLoading());

  // Then the definition opens, scrolled to its line, and the notice is gone
  expectTheDefinitionOfAreaOpen();
  worktreeCodePanePage.notice().should("not.exist");
});

it("a held go-to-definition reports that the index failed to load, and why", () => {
  // Given a definition of `area` held by the loading index
  const host = aGeometryWorktreeHost().startingAt(anIndexLoading(INDEXING_38));
  const pane = aCodePaneOn(host).mount().previewing("src/main.rs");
  pane.goToDefinitionOf(THE_CALL_TO_AREA);
  worktreeCodePanePage.notice().should("have.text", "Index loading — Indexing 38%");

  // When the index fails to load
  cy.then(() => host.failLoading("rust-analyzer exited: no toolchain"));

  // Then the notice is an alert naming the failure
  worktreeCodePanePage
    .notice()
    .should("have.attr", "role", "alert")
    .and(
      "have.text",
      "Go to definition failed: the code index failed: rust-analyzer exited: no toolchain",
    );
});

it("says no definition found only for an answer from a loaded index", () => {
  // Given main.rs previewed with the index loaded, and nothing defines `main` for it
  const host = aGeometryWorktreeHost().startingAt(anIndexReady());
  const pane = aCodePaneOn(host).mount().previewing("src/main.rs");

  // When going to the definition of `main`
  pane.goToDefinitionOf(MAIN);

  // Then the loaded index's empty answer reads as no definition
  expectTheHost(host).toHaveBeenAskedOfKind("DEFINITION", [{ kind: "DEFINITION", ...inMainRs(MAIN) }]);
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
  // (waits for `main`'s probe, so the second dwell is a separate one)
  expectTheHost(host).toHaveBeenAskedOfKind("PROBE", [{ kind: "PROBE", ...inMainRs(MAIN) }]);
  pane.dwellOn(THE_CALL_TO_AREA);

  // Then `area` is underlined with a pointer cursor
  expectUnderlined(THE_CALL_TO_AREA);
  // And `main` stays plain
  expectPlain(MAIN);
});

it("probes a position once however often the pointer rests on it", () => {
  // Given main.rs previewed with the index loaded, and the call to `area` underlined after a dwell
  const host = aGeometryWorktreeHost().startingAt(anIndexReady());
  const pane = aCodePaneOn(host).mount().previewing("src/main.rs");
  pane.dwellOn(THE_CALL_TO_AREA);
  expectUnderlined(THE_CALL_TO_AREA);

  // When the pointer rests on it again
  pane.dwellOn(THE_CALL_TO_AREA);

  // Then its hover was asked twice but its probe only once
  expectTheHost(host).toHaveBeenAskedOfKind("HOVER", [
    { kind: "HOVER", ...inMainRs(THE_CALL_TO_AREA) },
    { kind: "HOVER", ...inMainRs(THE_CALL_TO_AREA) },
  ]);
  expectTheHost(host).toHaveBeenAskedOfKind("PROBE", [
    { kind: "PROBE", ...inMainRs(THE_CALL_TO_AREA) },
  ]);
});

it("a probe the loading index cannot answer leaves the identifier plain and is asked again once it is ready", () => {
  // Given main.rs previewed while the index is indexing, the pointer resting on the call to `area`
  const host = aGeometryWorktreeHost().startingAt(anIndexLoading(INDEXING_38));
  const pane = aCodePaneOn(host).mount().previewing("src/main.rs");
  pane.dwellOn(THE_CALL_TO_AREA);
  // (the held hover follows the probe on the stream, so its probe has been answered by now)
  worktreeCodePanePage.hoverLoading().should("be.visible");

  // Then the probe's "unknown" leaves `area` plain
  expectPlain(THE_CALL_TO_AREA);

  // When the index finishes loading and the pointer rests on `area` again
  cy.then(() => host.finishLoading());
  worktreeCodePanePage.hoverBody().should("have.text", AREA_TYPE);
  pane.dwellOn(THE_CALL_TO_AREA);

  // Then it was probed again, and is underlined now
  expectTheHost(host).toHaveBeenAskedOfKind("PROBE", [
    { kind: "PROBE", ...inMainRs(THE_CALL_TO_AREA) },
    { kind: "PROBE", ...inMainRs(THE_CALL_TO_AREA) },
  ]);
  expectUnderlined(THE_CALL_TO_AREA);
});

it("opening another file cancels the probes in flight, and their late answers underline nothing", () => {
  // Given main.rs previewed with the index loaded, and a probe of the call to `area` in flight
  const host = aGeometryWorktreeHost().startingAt(anIndexReady()).holdingProbeAnswers();
  const pane = aCodePaneOn(host).mount().previewing("src/main.rs");
  pane.dwellOn(THE_CALL_TO_AREA);
  expectTheHost(host).toHaveBeenAskedOfKind("PROBE", [
    { kind: "PROBE", ...inMainRs(THE_CALL_TO_AREA) },
  ]);

  // When another file is opened, then main.rs again, and only then the probe is answered
  pane.opening("src/geometry.rs").opening("src/main.rs");
  cy.then(() => host.answerHeldProbes());

  // Then the pane cancelled the probe when it left the file
  expectTheHost(host).toHaveCancelled([{ kind: "PROBE", ...inMainRs(THE_CALL_TO_AREA) }]);
  // And a fresh dwell probes `area` again rather than trusting the stale answer
  pane.dwellOn(THE_CALL_TO_AREA);
  expectTheHost(host).toHaveBeenAskedOfKind("PROBE", [
    { kind: "PROBE", ...inMainRs(THE_CALL_TO_AREA) },
    { kind: "PROBE", ...inMainRs(THE_CALL_TO_AREA) },
  ]);
  // And `area` stays plain until that probe is answered
  expectPlain(THE_CALL_TO_AREA);
  cy.then(() => host.answerHeldProbes());
  expectUnderlined(THE_CALL_TO_AREA);
});

it("a plain click on an underlined identifier opens its definition", () => {
  // Given main.rs previewed with the index loaded, and the call to `area` underlined after a dwell
  const host = aGeometryWorktreeHost().startingAt(anIndexReady());
  const pane = aCodePaneOn(host).mount().previewing("src/main.rs");
  pane.dwellOn(THE_CALL_TO_AREA);
  expectUnderlined(THE_CALL_TO_AREA);

  // When clicking it with no modifier key
  pane.plainClick(THE_CALL_TO_AREA);

  // Then the definition opens, scrolled to its line
  expectTheDefinitionOfAreaOpen();
  // And it went where the probe pointed, without asking for the definition again
  expectTheHost(host).toHaveBeenAskedOfKind("DEFINITION", []);
});

it("a plain click on an identifier without a definition asks nothing", () => {
  // Given main.rs previewed with the index loaded, and `main` probed and found to have no definition
  const host = aGeometryWorktreeHost().startingAt(anIndexReady());
  const pane = aCodePaneOn(host).mount().previewing("src/main.rs");
  pane.dwellOn(MAIN);
  expectTheHost(host).toHaveBeenAskedOfKind("PROBE", [{ kind: "PROBE", ...inMainRs(MAIN) }]);

  // When clicking `main` with no modifier key, and then ctrl-clicking it
  pane.plainClick(MAIN).goToDefinitionOf(MAIN);

  // Then only the ctrl-click asked for a definition — frames arrive in order on the one stream, so
  // a definition asked by the plain click would have come first
  expectTheHost(host).toHaveBeenAskedOfKind("DEFINITION", [{ kind: "DEFINITION", ...inMainRs(MAIN) }]);
  worktreeCodePanePage.notice().should("have.text", "No definition found");
});

// ---------------------------------------------------------------------------
// Closing
// ---------------------------------------------------------------------------

it("closing the pane ends its code session", () => {
  // Given an open pane holding its code session
  const host = aGeometryWorktreeHost().startingAt(anIndexReady());
  const pane = aCodePaneOn(host).mount();
  expectTheHost(host).toHaveOpenAndEndedSessions(1, 0);

  // When the pane closes
  pane.close();

  // Then its code session has ended
  expectTheHost(host).toHaveOpenAndEndedSessions(0, 1);
});
