/**
 * Acceptance: the Agent Activity overlay loads **lazily** and its state **persists across a session
 * switch**.
 *
 * - Requirement #2 (lazy, count-first): while a session is focused the overlay subscribes in
 *   `COUNT_THEN_LIVE` and shows its icon + unread badge from the streamed **count** alone — it does
 *   NOT pull the full transcript. The heavy `SNAPSHOT_THEN_LIVE` snapshot is fetched only when the
 *   overlay is first opened ("visited once"), and only once.
 * - Requirement #1 (persistence): the transcript + count live in a module-level per-session store, so
 *   switching to another session and back reuses the cached state instead of re-pulling the snapshot.
 *
 * These mount the self-contained `AgentActivityOverlay` over an in-memory backend whose
 * `StreamAcpReplay` handler answers the two modes separately and tallies how many times each mode was
 * opened (`opens.count` / `opens.snapshot`) — the streaming testkit records unary calls only, so the
 * backend exposes its own open-counters.
 *
 * PRD: docs/ft/web/agent-activity-pane.md § Persisted, lazily-counted activity.
 */

import React from "react";
import { AgentActivityOverlay } from "../../src/components/sessions/AgentActivityOverlay";
import { mountWithRpc } from "../support/rpc/inMemory";
import { agentActivityPage } from "../support/pages/agentActivityPage";
import { agentChatPage } from "../support/pages/agentChatPage";
import { aReplayBackend, replayAgentText } from "../support/rpc/acpReplay";

type Backend = ReturnType<typeof aReplayBackend>["backend"];

function mountOverlay(backend: Backend, sessionId: string) {
  mountWithRpc(
    <AgentActivityOverlay sessionId={sessionId} sessionToken="tok" sessionType="tool" />,
    backend,
  );
}

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("shows the icon and unread count from the count feed without pulling the snapshot", () => {
  // Given — the count feed reports 3 activities; a full snapshot exists but must not be pulled yet
  const { backend, opens } = aReplayBackend({
    counts: [3],
    snapshot: [replayAgentText("Full transcript body.", 1_000)],
  });

  // When — the overlay mounts for a focused session (overlay closed)
  mountOverlay(backend, "lazy-icon");

  // Then — the icon and its unread badge come from the count alone; the snapshot stream is untouched
  agentActivityPage.button().should("exist");
  agentActivityPage.unreadBadge().should("have.text", "3");
  cy.wrap(opens).its("count").should("equal", 1);
  cy.wrap(opens).its("snapshot").should("equal", 0);
});

it("pulls the full transcript lazily on first open, exactly once", () => {
  // Given — one counted activity and a one-line transcript
  const { backend, opens } = aReplayBackend({
    counts: [1],
    snapshot: [replayAgentText("Analyzing the parser.", 1_000)],
  });

  // When — the overlay mounts: it subscribes to the count feed but leaves the snapshot untouched
  mountOverlay(backend, "lazy-open");
  cy.wrap(opens).its("count").should("equal", 1);
  cy.wrap(opens).its("snapshot").should("equal", 0);

  // …then the operator opens the overlay
  agentActivityPage.open();

  // Then — opening pulls the snapshot once and renders the transcript
  agentChatPage.chatMessage(0).should("have.text", "Analyzing the parser.");
  cy.wrap(opens).its("snapshot").should("equal", 1);
});

it("raises the unread badge as live activity increments the count", () => {
  // Given — the count feed climbs 1 → 2 → 3 as activity lands (no transcript payload)
  const { backend } = aReplayBackend({ counts: [1, 2, 3] });

  // When — the overlay mounts and stays closed
  mountOverlay(backend, "lazy-increment");

  // Then — the badge reflects the latest streamed count
  agentActivityPage.unreadBadge().should("have.text", "3");
});

it("clears the unread badge once the overlay is opened", () => {
  // Given — the count feed reports 5 activities; the (lazily-pulled) transcript holds fewer entries,
  // so the badge must reflect the streamed COUNT (5), not the number of rendered messages
  const { backend } = aReplayBackend({
    counts: [5],
    snapshot: [replayAgentText("one", 1_000), replayAgentText("two", 2_000)],
  });

  // When — the badge shows the streamed count, then the operator opens the overlay (marks it seen)
  mountOverlay(backend, "lazy-seen");
  agentActivityPage.unreadBadge().should("have.text", "5");
  agentActivityPage.open();

  // Then — the unread badge is cleared
  agentActivityPage.unreadBadge({ timeout: 1000 }).should("not.exist");
});

it("retains the transcript across a session switch without re-pulling the snapshot", () => {
  // Given — a harness that switches the focused session between two ids and back
  const { backend, opens } = aReplayBackend({
    counts: [1],
    snapshot: [replayAgentText("Session A transcript.", 1_000)],
  });

  function SwitchHarness() {
    const [sessionId, setSessionId] = React.useState("switch-A");
    return (
      <div>
        <button data-testid="switch-to-a" onClick={() => setSessionId("switch-A")}>
          A
        </button>
        <button data-testid="switch-to-b" onClick={() => setSessionId("switch-B")}>
          B
        </button>
        <AgentActivityOverlay sessionId={sessionId} sessionToken="tok" sessionType="tool" />
      </div>
    );
  }

  // When — open A's overlay (pulls A's snapshot once), then switch away to B and back to A
  mountWithRpc(<SwitchHarness />, backend);
  agentActivityPage.open();
  agentChatPage.chatMessage(0).should("have.text", "Session A transcript.");
  cy.wrap(opens).its("snapshot").should("equal", 1);
  agentActivityPage.close();
  cy.get("[data-testid='switch-to-b']").click();
  cy.get("[data-testid='switch-to-a']").click();
  agentActivityPage.open();

  // Then — A's transcript is still there, restored from the per-session store, not re-pulled
  agentChatPage.chatMessage(0).should("have.text", "Session A transcript.");
  cy.wrap(opens).its("snapshot").should("equal", 1);
});
