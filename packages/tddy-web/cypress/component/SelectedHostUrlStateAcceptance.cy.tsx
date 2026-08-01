/**
 * Acceptance tests: the selected host (daemon) lives in the URL, so a reload — or a link opened in
 * a fresh tab, which has no `sessionStorage` — lands on the host the link names.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-01-url-state-routing.md § Host in the URL.
 * Changeset: docs/dev/1-WIP/2026-08-01-web-url-state-routing.md.
 *
 * All RPC calls flow through the in-memory backend — no HTTP intercepts.
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import type { DaemonHost } from "../../src/lib/participantRole";
import { SELECTED_DAEMON_STORAGE_KEY } from "../../src/routing/selectedHost";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { daemonSelectorPage } from "../support/pages/daemonSelectorPage";
import { appLocationPage } from "../support/pages/appLocationPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const UDOO: DaemonHost = { instanceId: "udoo", label: "udoo (this daemon)" };
const LAPTOP_B: DaemonHost = { instanceId: "laptop-b", label: "laptop-b" };

const SESSION = {
  sessionId: "host-url-000000-0000-0000-000000000001",
  createdAt: "2026-08-01T09:00:00Z",
  status: "exited",
  repoPath: "/home/dev/host-url",
  pid: 0,
  isActive: false,
  projectId: "proj-host-url",
  daemonInstanceId: "",
  workflowGoal: "Host URL state",
  pendingElicitation: false,
  orchestratorSessionId: "",
  recipe: "",
  sessionType: "claude-cli",
};

/** Mount the sessions drawer with both fixture daemons available in the common room. */
function mountWithBothHosts() {
  mountWithRpc(
    withSelectedDaemon(<SessionsDrawerScreen />, [UDOO, LAPTOP_B]),
    aSessionsDrawerBackend([SESSION]),
  );
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
  appLocationPage.reset();
});

// ---------------------------------------------------------------------------
// Writing the host
// ---------------------------------------------------------------------------

it("choosing a host records it in the URL", () => {
  // Given
  mountWithBothHosts();

  // When
  daemonSelectorPage.choose("laptop-b");

  // Then
  appLocationPage.expectParam("host", "laptop-b");
});

it("the resolved host is written back into a URL that carried none", () => {
  // Given — a link with no host param at all
  appLocationPage.startAt("/sessions");

  // When
  mountWithBothHosts();

  // Then — the first available daemon is resolved and recorded, so the address bar is shareable
  appLocationPage.expectParam("host", "udoo");
});

it("switching host drops the selected session from the URL", () => {
  // Given — a session selected on the first host
  mountWithBothHosts();
  sessionsDrawerPage.drawerItem(SESSION.sessionId).click();
  appLocationPage.expectPath(`/sessions/${SESSION.sessionId}`);

  // When — the operator switches to a host that does not own that session
  daemonSelectorPage.choose("laptop-b");

  // Then — back to the drawer root on the new host
  appLocationPage.expectPath("/sessions");
  appLocationPage.expectParam("host", "laptop-b");
});

// ---------------------------------------------------------------------------
// Reading the host
// ---------------------------------------------------------------------------

it("a ?host= deep link selects that host over the one stored for this tab", () => {
  // Given — this tab last used udoo, but the link names laptop-b
  window.sessionStorage.setItem(SELECTED_DAEMON_STORAGE_KEY, "udoo");
  appLocationPage.startAt("/sessions?host=laptop-b");

  // When
  mountWithBothHosts();

  // Then
  daemonSelectorPage.expectShowsSelected("laptop-b");
});

it("falls back to the stored host when the URL names a daemon that is not in the room", () => {
  // Given — a link naming a daemon that has since left the common room
  window.sessionStorage.setItem(SELECTED_DAEMON_STORAGE_KEY, "laptop-b");
  appLocationPage.startAt("/sessions?host=retired-host");

  // When
  mountWithBothHosts();

  // Then
  daemonSelectorPage.expectShowsSelected("laptop-b");
});

it("a ?host= deep link selects that host in a tab that has no stored selection", () => {
  // Given — a fresh tab (nothing in sessionStorage), opened from a shared link
  appLocationPage.startAt("/sessions?host=laptop-b");

  // When
  mountWithBothHosts();

  // Then
  daemonSelectorPage.expectShowsSelected("laptop-b");
});
