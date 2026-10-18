/**
 * The session header keeps watching its code index for as long as it is mounted. Its
 * `code_navigation.WatchCodeIndex` never starts an index daemon, so with none running a watch ends
 * at once and empty — and one also ends when the index daemon stops. The header watches again after
 * the Code pane's reconnect waits (1 s, doubling to 30 s), so a load the pane or the agent starts
 * later still shows; a watch that ended drops what it last said; a refusal is not asked again.
 *
 * Feature: docs/ft/web/1-WIP/PRD-2026-10-18-code-index-warm-on-demand.md (session header);
 * changeset docs/dev/1-WIP/2026-10-18-code-index-warm-on-demand.md (FV2, FV6).
 *
 * Timing is pinned with `cy.clock`, so every wait is exact and nothing waits in real time.
 */

import { createClient } from "@connectrpc/connect";
import { CodeNavigationService } from "../../src/gen/code_navigation_pb";
import { SessionIndexingIndicator } from "../../src/components/session/SessionIndexingIndicator";
import { mountWithRpc } from "../support/rpc/inMemory";
import { anIndexLoading } from "../support/rpc/codeSessionBackend";
import {
  aSessionIndexWatchHost,
  expectTheHeaderToHaveStillWatched,
  expectTheHeaderToHaveWatched,
  type SessionIndexWatchHost,
} from "../support/rpc/sessionIndexWatchBackend";
import { INDEXING_38 } from "../support/fixtures/geometryWorktree";
import { sessionIndexingIndicatorPage } from "../support/pages/sessionIndexingIndicatorPage";

const INDEXING_38_TEXT = "Indexing — Indexing 38%";

function mountTheHeaderOn(host: SessionIndexWatchHost) {
  const backend = host.backend();
  mountWithRpc(
    <SessionIndexingIndicator
      client={createClient(CodeNavigationService, backend.transport())}
      sessionToken="fake-token"
      sessionId="session-indexing-1"
    />,
    backend,
  );
}

beforeEach(() => {
  cy.viewport(1280, 900);
});

it("the header shows a load that starts after its first watch ended empty", () => {
  // Given a header on a host with no index daemon running, whose first watch has ended empty
  cy.clock();
  const host = aSessionIndexWatchHost();
  mountTheHeaderOn(host);
  expectTheHeaderToHaveWatched(host, 1);
  sessionIndexingIndicatorPage.indicator().should("not.exist");

  // When the Code pane starts the index loading, and a second passes
  cy.then(() => host.indexDaemonStartsLoading(anIndexLoading(INDEXING_38)));
  cy.tick(1_000);

  // Then the header has watched again and shows the load
  expectTheHeaderToHaveWatched(host, 2);
  sessionIndexingIndicatorPage.indicator().should("have.text", INDEXING_38_TEXT);
});

it("the header drops what it last said once its watch ends mid-load", () => {
  // Given a header showing a load in progress
  cy.clock();
  const host = aSessionIndexWatchHost().withTheIndexDaemonAt(anIndexLoading(INDEXING_38));
  mountTheHeaderOn(host);
  sessionIndexingIndicatorPage.indicator().should("have.text", INDEXING_38_TEXT);

  // When the index daemon stops, ending the watch
  cy.then(() => host.indexDaemonStops());

  // Then the stale indexing text is gone, before any new watch has reported
  sessionIndexingIndicatorPage.indicator().should("not.exist");
  expectTheHeaderToHaveStillWatched(host, 1);
});

it("the header watches again after 1, 2 and 4 seconds while no index daemon runs", () => {
  // Given a header on a host with no index daemon running
  cy.clock();
  const host = aSessionIndexWatchHost();
  mountTheHeaderOn(host);
  expectTheHeaderToHaveWatched(host, 1);

  // Then each empty watch is followed by the next after a wait twice the last
  cy.tick(999);
  expectTheHeaderToHaveStillWatched(host, 1);
  cy.tick(1);
  expectTheHeaderToHaveWatched(host, 2);
  cy.tick(1_999);
  expectTheHeaderToHaveStillWatched(host, 2);
  cy.tick(1);
  expectTheHeaderToHaveWatched(host, 3);
  cy.tick(3_999);
  expectTheHeaderToHaveStillWatched(host, 3);
  cy.tick(1);
  expectTheHeaderToHaveWatched(host, 4);
});

it("a watch the host refuses is not asked again, and the header names why", () => {
  // Given a host that refuses every watch of the session's index
  cy.clock();
  const host = aSessionIndexWatchHost().refusingEveryWatch("the session is not yours");

  // When the header mounts and a minute passes
  mountTheHeaderOn(host);
  sessionIndexingIndicatorPage.indicator().should("exist");
  cy.tick(60_000);

  // Then it watched once, and names the refusal
  expectTheHeaderToHaveStillWatched(host, 1);
  sessionIndexingIndicatorPage
    .indicator()
    .should("have.text", "Indexing failed — the session is not yours");
});
