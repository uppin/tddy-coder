/**
 * Acceptance tests: Usage tab in the session inspector drawer.
 *
 * PRD: docs/ft/web/session-usage-inspector.md.
 *
 * Real-time per-session token usage streams over the existing `TddyRemote.Stream` as a
 * `tokenUsageUpdated` `ServerMessage` carrying the full cumulative per-conversation snapshot.
 * Exercised through the full `SessionsDrawerScreen`; all RPC (including the LiveKit-transport
 * presenter stream) flows through an in-memory backend — no `cy.intercept`.
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionUsageBackend, aConversationRecord } from "../support/rpc/usageBackend";
import { sessionsDrawerPage as page } from "../support/pages/sessionsDrawerPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const SESSION = {
  sessionId: "usage-test-session-aabbccdd-0000-0000-0000-000000000001",
  createdAt: "2026-07-12T10:00:00Z",
  status: "active",
  repoPath: "/home/dev/usage-project",
  pid: 12345,
  isActive: true,
  projectId: "proj-usage-1",
  daemonInstanceId: "",
  workflowGoal: "Usage test session",
  pendingElicitation: false,
};

const CLAUDE = aConversationRecord({
  agent: "claude",
  id: "claude-main",
  model: "claude-opus-4-8",
  inputTokens: 12340n,
  outputTokens: 3210n,
  totalTokens: 15550n,
  turns: 7,
});

const EXPLORE = aConversationRecord({
  agent: "Explore",
  id: "agent-01",
  model: "claude-haiku-4-5",
  inputTokens: 4100n,
  outputTokens: 820n,
  totalTokens: 4920n,
  turns: 2,
});

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800); // desktop: session list defaults open so drawer items are clickable
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

// ---------------------------------------------------------------------------
// The Usage tab is present in the inspector tab strip
// ---------------------------------------------------------------------------

it("shows a Usage tab in the inspector tab strip alongside Details and Tools", () => {
  // Given
  const backend = aSessionUsageBackend([SESSION], []);

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  page.drawerItem(SESSION.sessionId).click();
  page.inspectorDrawer().should("have.attr", "data-state", "open");

  // Then
  page.inspectorUsageTab().should("exist");
});

// ---------------------------------------------------------------------------
// Selecting the Usage tab shows the streamed per-conversation breakdown
// ---------------------------------------------------------------------------

it("shows one row per conversation with agent, model and token counts when a snapshot streams", () => {
  // Given — a snapshot with the main agent and one subagent
  const backend = aSessionUsageBackend([SESSION], [[CLAUDE, EXPLORE]]);

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  page.drawerItem(SESSION.sessionId).click();
  page.inspectorUsageTab().click();

  // Then — the main-agent row
  page.usageRowAgent("claude-main").should("have.text", "claude");
  page.usageRowModel("claude-main").should("have.text", "claude-opus-4-8");
  page.usageRowInput("claude-main").should("have.text", "12,340");
  page.usageRowOutput("claude-main").should("have.text", "3,210");
  page.usageRowTotal("claude-main").should("have.text", "15,550");
  page.usageRowTurns("claude-main").should("have.text", "7");

  // …and the subagent row
  page.usageRowAgent("agent-01").should("have.text", "Explore");
  page.usageRowModel("agent-01").should("have.text", "claude-haiku-4-5");
  page.usageRowInput("agent-01").should("have.text", "4,100");
  page.usageRowOutput("agent-01").should("have.text", "820");
  page.usageRowTotal("agent-01").should("have.text", "4,920");
  page.usageRowTurns("agent-01").should("have.text", "2");
});

// ---------------------------------------------------------------------------
// TOTAL row sums every conversation
// ---------------------------------------------------------------------------

it("shows a TOTAL row summing input, output and total tokens across all conversations", () => {
  // Given
  const backend = aSessionUsageBackend([SESSION], [[CLAUDE, EXPLORE]]);

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  page.drawerItem(SESSION.sessionId).click();
  page.inspectorUsageTab().click();

  // Then — 12,340 + 4,100 = 16,440 in; 3,210 + 820 = 4,030 out; 15,550 + 4,920 = 20,470 total
  page.usageTotalInput().should("have.text", "16,440");
  page.usageTotalOutput().should("have.text", "4,030");
  page.usageTotalTotal().should("have.text", "20,470");
});

// ---------------------------------------------------------------------------
// A newer snapshot replaces the displayed totals
// ---------------------------------------------------------------------------

it("updates the TOTAL to the latest snapshot when the main agent's usage grows", () => {
  // Given — an initial snapshot, then a second one where the main agent has spent more
  const claudeLater = aConversationRecord({
    agent: "claude",
    id: "claude-main",
    model: "claude-opus-4-8",
    inputTokens: 20000n,
    outputTokens: 5000n,
    totalTokens: 25000n,
    turns: 10,
  });
  const backend = aSessionUsageBackend(
    [SESSION],
    [
      [CLAUDE, EXPLORE],
      [claudeLater, EXPLORE],
    ],
  );

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  page.drawerItem(SESSION.sessionId).click();
  page.inspectorUsageTab().click();

  // Then — latest totals: 20,000 + 4,100 = 24,100 in; 25,000 + 4,920 = 29,920 total
  page.usageTotalInput().should("have.text", "24,100");
  page.usageTotalTotal().should("have.text", "29,920");
});

// ---------------------------------------------------------------------------
// Zero state before any usage arrives
// ---------------------------------------------------------------------------

it("shows a zero state and no conversation rows before any usage snapshot arrives", () => {
  // Given — a session whose stream reports no usage
  const backend = aSessionUsageBackend([SESSION], []);

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  page.drawerItem(SESSION.sessionId).click();
  page.inspectorUsageTab().click();

  // Then
  page.usageEmpty().should("exist");
  page.usageRow("claude-main").should("not.exist");
});
