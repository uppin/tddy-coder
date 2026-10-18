/**
 * The Code pane's bottom strip shows where its worktree's code index stands, as the pane's
 * `code_navigation.CodeSession` reports it — the phase running now, its percentage and message
 * while it loads, "ready" once it is queryable, and why when the session failed.
 *
 * Feature: docs/ft/web/session-code-pane.md (Indexing indicator);
 * docs/ft/web/1-WIP/PRD-2026-10-18-code-index-warm-on-demand.md.
 */

import { aCodeIndexHost, anIndexLoading, type CodeIndexHost, type IndexPhase } from "../support/rpc/codeSessionBackend";
import { aCodePaneOn } from "../support/drivers/worktreeCodePaneDriver";
import { worktreeCodePanePage } from "../support/pages/worktreeCodePanePage";

const INDEXING_38: IndexPhase = { title: "Indexing", percentage: 38, message: "120/310 (tddy_core)" };
const FETCHING_10: IndexPhase = { title: "Fetching", percentage: 10, message: "crates.io index" };

function aHostWhoseIndexIs(phase: IndexPhase): CodeIndexHost {
  return aCodeIndexHost().withFile("src/main.rs", "fn main() {}\n").startingAt(anIndexLoading(phase));
}

beforeEach(() => {
  cy.viewport(1280, 900);
});

it("shows the phase running now and its message, then that the index is ready", () => {
  // Given a host whose index is indexing tddy_core
  const host = aHostWhoseIndexIs(INDEXING_38);

  // When the pane opens
  aCodePaneOn(host).mount();

  // Then the strip names the phase, its percentage and its message
  worktreeCodePanePage.indexStatus().should("have.attr", "data-state", "progress");
  worktreeCodePanePage.indexStatusSummary().should("have.text", "Indexing — Indexing 38%");
  worktreeCodePanePage.indexStatusDetail().should("have.text", "120/310 (tddy_core)");

  // When the index becomes ready
  cy.then(() => host.finishLoading());

  // Then the strip stays, saying so
  worktreeCodePanePage.indexStatus().should("have.attr", "data-state", "ready");
  worktreeCodePanePage.indexStatusSummary().should("have.text", "Code index ready");
  worktreeCodePanePage.indexStatusDetail().should("not.exist");
});

it("names the reason when the code session fails", () => {
  // Given a host whose index is fetching
  const host = aHostWhoseIndexIs(FETCHING_10);
  aCodePaneOn(host).mount();
  worktreeCodePanePage.indexStatus().should("have.attr", "data-state", "progress");

  // When the code session fails
  cy.then(() => host.failSessions("index daemon unreachable"));

  // Then the strip names the failure
  worktreeCodePanePage.indexStatus().should("have.attr", "data-state", "failed");
  worktreeCodePanePage
    .indexStatusSummary()
    .should("contain.text", "Indexing failed")
    .and("contain.text", "index daemon unreachable");
});
