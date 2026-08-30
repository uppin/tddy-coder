/**
 * Acceptance: the session drawer turns notifications into indicators.
 *
 * PRD: docs/ft/daemon/session-notifications.md (AC6–AC10).
 *
 * A drawer row's dot has four states, and every one of them is a claim about what the operator
 * should do next: grey (the session is gone), steady green (alive, nothing new), blinking green
 * (the agent is working right now), yellow (it is waiting on you). The first two are already
 * derived from `ListSessions`; the last two arrive on the daemon-level
 * `StreamSessionNotifications` feed, which is why these specs drive the feed rather than the
 * session list.
 *
 * `ConnectionService` is daemon-level RPC (`useDaemonClient`), routed over the shared common-room
 * LiveKit connection — see `aConnectionServiceBackend` (in-memory fake) and `withSelectedDaemon`.
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aConnectionServiceBackend } from "../support/rpc/connectionServiceBackend";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import {
  aSessionNotificationFeed,
  anActivityNotification,
  anAttentionNotification,
  type SessionNotificationFeed,
} from "../support/rpc/sessionNotificationFeed";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/** An alive session whose drawer label is its worktree basename — `my-feature-branch`. */
const WORKING_SESSION = {
  sessionId: "indicator-working-aaaaaaaa-0000-0000-0000-000000000001",
  createdAt: "2026-08-29T12:00:00Z",
  status: "active",
  repoPath: "/home/dev/my-feature-branch",
  pid: 40001,
  isActive: true,
  projectId: "proj-indicators",
  daemonInstanceId: "",
  workflowGoal: "Revamp the session notifications",
  pendingElicitation: false,
};
const WORKING_SESSION_LABEL = "my-feature-branch";

/** A second alive session, so a spec can state that one row's news leaves the other alone. */
const QUIET_SESSION = {
  sessionId: "indicator-quiet-bbbbbbbb-0000-0000-0000-000000000002",
  createdAt: "2026-08-29T11:00:00Z",
  status: "active",
  repoPath: "/home/dev/quiet-branch",
  pid: 40002,
  isActive: true,
  projectId: "proj-indicators",
  daemonInstanceId: "",
  workflowGoal: "Nothing happening here",
  pendingElicitation: false,
};

/** A session whose process is gone. Its dot is grey whatever the feed says about it. */
const ENDED_SESSION = {
  sessionId: "indicator-ended-cccccccc-0000-0000-0000-000000000003",
  createdAt: "2026-08-29T10:00:00Z",
  status: "exited",
  repoPath: "/home/dev/ended-branch",
  pid: 0,
  isActive: false,
  projectId: "proj-indicators",
  daemonInstanceId: "",
  workflowGoal: "Finished long ago",
  pendingElicitation: false,
};
const ENDED_SESSION_LABEL = "ended-branch";

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/** Mount the drawer against a live notification feed the spec drives by hand. */
function aDrawerWatchingNotifications(
  sessions: Array<typeof WORKING_SESSION>,
): SessionNotificationFeed {
  const notifications = aSessionNotificationFeed();
  const backend = aConnectionServiceBackend({ sessions, sessionNotifications: notifications });
  mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  // The feed is push-driven, so a spec must not push before the screen has subscribed. This gate
  // retries until the one subscription the drawer opens is live.
  cy.wrap(null, { log: false }).should(() => {
    expect(notifications.subscriptionCount()).to.equal(1);
  });
  return notifications;
}

beforeEach(() => {
  cy.viewport(1280, 800); // desktop: the session list defaults open
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

// ---------------------------------------------------------------------------

describe("SessionsDrawerScreen — notifications as per-session indicators", () => {
  // -------------------------------------------------------------------------
  // AC6 — blinking green means the agent is working
  // -------------------------------------------------------------------------

  it("blinks a session's dot green while the notification stream reports agent activity", () => {
    // Given — an alive session with nothing new, showing a steady green dot
    const notifications = aDrawerWatchingNotifications([WORKING_SESSION]);
    sessionsDrawerPage.expectIndicator(WORKING_SESSION.sessionId, "connected");
    sessionsDrawerPage.expectIndicatorSteady(WORKING_SESSION.sessionId);

    // When — the daemon reports the agent working
    cy.then(() => {
      notifications.push(
        anActivityNotification(WORKING_SESSION.sessionId, WORKING_SESSION_LABEL),
      );
    });

    // Then — the dot stays green and starts fading in and out
    sessionsDrawerPage.expectIndicator(WORKING_SESSION.sessionId, "working");
    sessionsDrawerPage.expectIndicatorBlinking(WORKING_SESSION.sessionId);
  });

  // -------------------------------------------------------------------------
  // AC7 — yellow means attention is required
  // -------------------------------------------------------------------------

  it("turns a session's dot yellow when the notification stream reports attention required", () => {
    // Given
    const notifications = aDrawerWatchingNotifications([WORKING_SESSION]);
    sessionsDrawerPage.expectIndicator(WORKING_SESSION.sessionId, "connected");

    // When — the same event that pings Telegram lands
    cy.then(() => {
      notifications.push(
        anAttentionNotification(WORKING_SESSION.sessionId, WORKING_SESSION_LABEL),
      );
    });

    // Then
    sessionsDrawerPage.expectIndicator(WORKING_SESSION.sessionId, "needs-input");
    sessionsDrawerPage.expectIndicatorSteady(WORKING_SESSION.sessionId);
  });

  it("prefers the yellow dot over the blinking one when the agent works and then asks", () => {
    // Given — a session that is already working
    const notifications = aDrawerWatchingNotifications([WORKING_SESSION]);
    cy.then(() => {
      notifications.push(
        anActivityNotification(WORKING_SESSION.sessionId, WORKING_SESSION_LABEL),
      );
    });
    sessionsDrawerPage.expectIndicator(WORKING_SESSION.sessionId, "working");

    // When — it stops to ask the operator something
    cy.then(() => {
      notifications.push(
        anAttentionNotification(WORKING_SESSION.sessionId, WORKING_SESSION_LABEL),
      );
    });

    // Then — "answer me" outranks "I am busy"
    sessionsDrawerPage.expectIndicator(WORKING_SESSION.sessionId, "needs-input");
    sessionsDrawerPage.expectIndicatorSteady(WORKING_SESSION.sessionId);
  });

  // -------------------------------------------------------------------------
  // AC8 — viewing the session settles its dot
  // -------------------------------------------------------------------------

  it("settles the dot to steady green once the operator selects the session", () => {
    // Given — a session asking for attention
    const notifications = aDrawerWatchingNotifications([WORKING_SESSION]);
    cy.then(() => {
      notifications.push(
        anAttentionNotification(WORKING_SESSION.sessionId, WORKING_SESSION_LABEL),
      );
    });
    sessionsDrawerPage.expectIndicator(WORKING_SESSION.sessionId, "needs-input");

    // When — the operator opens it
    sessionsDrawerPage.drawerItem(WORKING_SESSION.sessionId).click();

    // Then
    sessionsDrawerPage.expectSelected(WORKING_SESSION.sessionId);
    sessionsDrawerPage.expectIndicator(WORKING_SESSION.sessionId, "connected");
    sessionsDrawerPage.expectIndicatorSteady(WORKING_SESSION.sessionId);
  });

  it("raises the blinking dot again when activity lands after the session was selected", () => {
    // Given — a session the operator has already looked at
    const notifications = aDrawerWatchingNotifications([WORKING_SESSION]);
    cy.then(() => {
      notifications.push(
        anAttentionNotification(WORKING_SESSION.sessionId, WORKING_SESSION_LABEL),
      );
    });
    sessionsDrawerPage.expectIndicator(WORKING_SESSION.sessionId, "needs-input");
    sessionsDrawerPage.drawerItem(WORKING_SESSION.sessionId).click();
    sessionsDrawerPage.expectIndicator(WORKING_SESSION.sessionId, "connected");

    // When — the agent gets back to work
    cy.then(() => {
      notifications.push(
        anActivityNotification(WORKING_SESSION.sessionId, WORKING_SESSION_LABEL),
      );
    });

    // Then
    sessionsDrawerPage.expectIndicator(WORKING_SESSION.sessionId, "working");
    sessionsDrawerPage.expectIndicatorBlinking(WORKING_SESSION.sessionId);
  });

  // -------------------------------------------------------------------------
  // AC9 — one session's news is one session's news
  // -------------------------------------------------------------------------

  it("leaves every other row untouched when one session raises attention", () => {
    // Given — two alive sessions
    const notifications = aDrawerWatchingNotifications([WORKING_SESSION, QUIET_SESSION]);
    sessionsDrawerPage.expectIndicator(QUIET_SESSION.sessionId, "connected");

    // When — only one of them asks for the operator
    cy.then(() => {
      notifications.push(
        anAttentionNotification(WORKING_SESSION.sessionId, WORKING_SESSION_LABEL),
      );
    });
    sessionsDrawerPage.expectIndicator(WORKING_SESSION.sessionId, "needs-input");

    // Then — the other row is exactly as it was
    sessionsDrawerPage.expectIndicator(QUIET_SESSION.sessionId, "connected");
    sessionsDrawerPage.expectIndicatorSteady(QUIET_SESSION.sessionId);
  });

  // -------------------------------------------------------------------------
  // AC10 — a dead session is grey, whatever it last said
  // -------------------------------------------------------------------------

  it("keeps an inactive session's dot grey however its notifications read", () => {
    // Given — a session whose process is gone, listed alongside a live one
    const notifications = aDrawerWatchingNotifications([WORKING_SESSION, ENDED_SESSION]);
    sessionsDrawerPage.expandRemaining();
    sessionsDrawerPage.expectIndicator(ENDED_SESSION.sessionId, "disconnected");

    // When — a late notification arrives for it (the agent died mid-turn). The live session is
    // notified in the same batch: its dot turning green is the proof that the dead session's
    // notification was delivered too, which asserting only on the unchanged dot cannot show.
    cy.then(() => {
      notifications.push(anActivityNotification(ENDED_SESSION.sessionId, ENDED_SESSION_LABEL));
      notifications.push(anActivityNotification(WORKING_SESSION.sessionId, WORKING_SESSION_LABEL));
    });
    sessionsDrawerPage.expectIndicator(WORKING_SESSION.sessionId, "working");

    // Then — liveness decides first; a dead session never claims to be working
    sessionsDrawerPage.expectIndicator(ENDED_SESSION.sessionId, "disconnected");
    sessionsDrawerPage.expectIndicatorSteady(ENDED_SESSION.sessionId);
  });

  // -------------------------------------------------------------------------
  // The collapsed strip shows the same indicator as the expanded list
  // -------------------------------------------------------------------------

  it("blinks the collapsed strip's dot for a session the stream reports as working", () => {
    // Given — the drawer collapsed to its strip, where the dot is all a session shows
    const notifications = aDrawerWatchingNotifications([WORKING_SESSION]);
    sessionsDrawerPage.drawerCloseBtn().click();
    sessionsDrawerPage.expectIndicator(WORKING_SESSION.sessionId, "connected");

    // When
    cy.then(() => {
      notifications.push(
        anActivityNotification(WORKING_SESSION.sessionId, WORKING_SESSION_LABEL),
      );
    });

    // Then — the strip states the same thing the expanded row would
    sessionsDrawerPage.expectIndicator(WORKING_SESSION.sessionId, "working");
    sessionsDrawerPage.expectIndicatorBlinking(WORKING_SESSION.sessionId);
  });

  // -------------------------------------------------------------------------
  // NFR1 — one subscription for the whole drawer
  // -------------------------------------------------------------------------

  it("opens one notification stream for a drawer of many sessions", () => {
    // Given / When
    const notifications = aDrawerWatchingNotifications([
      WORKING_SESSION,
      QUIET_SESSION,
      ENDED_SESSION,
    ]);
    sessionsDrawerPage.expectIndicator(WORKING_SESSION.sessionId, "connected");

    // Then — three rows, one subscription
    cy.then(() => {
      expect(notifications.subscriptionCount()).to.equal(1);
    });
  });
});
