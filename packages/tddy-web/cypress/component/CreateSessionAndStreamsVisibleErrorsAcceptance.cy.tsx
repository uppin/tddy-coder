/**
 * Acceptance: the new-session form's pickers and the per-session status streams say why they are
 * empty or frozen, instead of swallowing the failure.
 *
 * Before, each of these swallowed its RPC failure — a `console.debug`, an empty `.catch`, or an
 * empty list on failure — so the operator saw a picker with nothing in it, a usage table stuck at
 * "nothing reported", or drawer dots that silently stopped moving, and could not tell "there is
 * nothing" from "the daemon refused". Each failure is now shown beside the surface it emptied, in
 * the daemon's own words ("<what> unavailable: <reason>"), the last good data stays shown, and the
 * message clears once that surface loads again.
 */

import React from "react";
import { Room } from "livekit-client";
import { create } from "@bufbuild/protobuf";
import { Code, ConnectError, createClient, type Client } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CreateSessionPane } from "../../src/components/sessions/CreateSessionPane";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { SessionUsageTab } from "../../src/components/sessions/SessionUsageTab";
import { useSessionActivity } from "../../src/components/sessions/useSessionActivity";
import { InlineError } from "../../src/components/ui/inline-error";
import { SessionService } from "../../src/gen/session_pb";
import { ProjectService } from "../../src/gen/project_pb";
import { CatalogService } from "../../src/gen/catalog_pb";
import { SessionFilesService } from "../../src/gen/session_files_pb";
import { WorktreeService } from "../../src/gen/worktree_pb";
import { ActivityService, AgentActivityRecordSchema } from "../../src/gen/activity_pb";
import { TddyRemote, ServerMessageSchema } from "../../src/gen/tddy/v1/remote_pb";
import type { DaemonHost } from "../../src/lib/participantRole";
import { SelectedDaemonProvider } from "../../src/rpc/selectedDaemon";
import { TEST_IDS } from "../support/testIds";
import { createSessionPage } from "../support/pages/createSessionPage";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { loadAndStreamErrorsPage as errors } from "../support/pages/loadAndStreamErrorsPage";
import { mountWithRpc } from "../support/rpc/inMemory";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aSessionServiceBackend } from "../support/rpc/daemonSessionHostBackend";
import { aConversationRecord } from "../support/rpc/usageBackend";
import {
  aSessionNotificationFeed,
  anAttentionNotification,
} from "../support/rpc/sessionNotificationFeed";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const LOCAL_HOST = "workstation-1";
const DAEMON_HOSTS: DaemonHost[] = [{ instanceId: LOCAL_HOST, label: "workstation-1 (this daemon)" }];

const ONLY_PROJECT = {
  projectId: "proj-only",
  name: "only",
  mainRepoPath: "/repo/only",
  daemonInstanceId: LOCAL_HOST,
};

const DRAWER_SESSION = {
  sessionId: "visible-errors-aaaaaaaa-0000-0000-0000-000000000001",
  createdAt: "2026-10-23T12:00:00Z",
  status: "active",
  repoPath: "/home/dev/visible-errors",
  pid: 40001,
  isActive: true,
  projectId: "proj-visible-errors",
  daemonInstanceId: "",
  workflowGoal: "Show the failures",
  pendingElicitation: false,
};
const DRAWER_SESSION_LABEL = "visible-errors";

/** Every RPC the new-session form issues, answering successfully with one project. */
function aCreateSessionBackend(): InMemoryRpcBackend {
  return anInMemoryRpcBackend()
    .onUnary(SessionService.method.listSessions, () => ({ sessions: [] }))
    .onUnary(CatalogService.method.listSubagents, () => ({ subagents: [] }))
    .onUnary(CatalogService.method.listAgentModels, () => ({
      models: [{ id: "claude-opus-4-8", label: "Claude Opus 4.8" }],
      defaultModel: "claude-opus-4-8",
    }))
    .onUnary(ProjectService.method.listProjects, () => ({ projects: [ONLY_PROJECT] }))
    .onUnary(CatalogService.method.listAgents, () => ({ agents: [{ id: "claude", label: "Claude" }] }))
    .onUnary(CatalogService.method.listTools, () => ({
      tools: [{ path: "/usr/bin/tddy-coder", label: "tddy-coder" }],
    }))
    .onUnary(ProjectService.method.listProjectBranches, () => ({
      branches: ["origin/master"],
      defaultRemote: "origin",
    }));
}

function mountCreatePane(backend: InMemoryRpcBackend) {
  const transport = backend.transport();
  cy.mount(
    <SelectedDaemonProvider room={new Room()} daemons={DAEMON_HOSTS} servingInstanceId={LOCAL_HOST}>
      <CreateSessionPane
        client={createClient(SessionService, transport)}
        projectClient={createClient(ProjectService, transport)}
        catalogClient={createClient(CatalogService, transport)}
        sessionFilesClient={createClient(SessionFilesService, transport)}
        worktreeClient={createClient(WorktreeService, transport)}
        sessionToken="fake-token"
        onCancel={cy.stub()}
        onCreated={cy.stub()}
      />
    </SelectedDaemonProvider>,
  );
}

/** A usage stream that delivers `snapshots` and then drops with `Unavailable` and `message`. */
function aUsageStreamThatDrops(
  snapshots: ReturnType<typeof aConversationRecord>[][],
  message: string,
): InMemoryRpcBackend {
  return anInMemoryRpcBackend().implement(TddyRemote, {
    stream: async function* () {
      for (const conversations of snapshots) {
        yield create(ServerMessageSchema, {
          event: { case: "tokenUsageUpdated", value: { conversations } },
        });
      }
      throw new ConnectError(message, Code.Unavailable);
    },
  });
}

/**
 * The smallest surface reading `useSessionActivity`: the hook has no production consumer today (the
 * overlay reads the ACP replay instead), so its failure contract is pinned through this harness.
 */
function ActivityStreamProbe({ client }: { client: Client<typeof ActivityService> }) {
  const { records, error } = useSessionActivity({
    sessionId: "activity-session",
    sessionToken: "fake-token",
    client,
  });
  return (
    <div>
      <span>{`${records.length} records`}</span>
      <InlineError
        testId={TEST_IDS.sessionActivityStreamError}
        message={error && `Activity unavailable: ${error}`}
      />
    </div>
  );
}

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

// ---------------------------------------------------------------------------
// New-session form pickers
// ---------------------------------------------------------------------------

describe("CreateSessionPane — picker loads that fail say why", () => {
  it("shows why the Project picker is empty when ListProjects fails", () => {
    // Given — the daemon refuses to list projects
    mountCreatePane(
      aCreateSessionBackend().failWith(
        ProjectService.method.listProjects,
        Code.Unavailable,
        "project registry is offline",
      ),
    );

    // Then — the picker says the projects are unavailable and why, not that there are none
    errors
      .createSessionProjectsError()
      .should("be.visible")
      .and("have.text", "Projects unavailable: project registry is offline");
  });

  it("still offers the projects, and says why there is no tool, when only ListTools fails", () => {
    // Given — the projects load, the tool catalog does not
    mountCreatePane(
      aCreateSessionBackend().failWith(
        CatalogService.method.listTools,
        Code.Internal,
        "tool catalog unreadable",
      ),
    );

    // When — the operator is starting a tool session
    createSessionPage.switchToToolSession();

    // Then — the project picker is unaffected and the missing tool is explained
    createSessionPage.awaitProjectOption(ONLY_PROJECT.projectId);
    errors
      .createSessionToolsError()
      .should("be.visible")
      .and("have.text", "Tools unavailable: tool catalog unreadable");
  });

  it("does not show a tools failure when the catalog loads", () => {
    // Given — every catalog answers
    mountCreatePane(aCreateSessionBackend());

    // When — the form has loaded
    createSessionPage.switchToToolSession();
    createSessionPage.awaitProjectOption(ONLY_PROJECT.projectId);

    // Then — nothing is reported
    errors.expectNoCreateSessionToolsError();
  });

  it("shows why the stack pickers have no sessions when ListSessions fails", () => {
    // Given — the session list is refused
    mountCreatePane(
      aCreateSessionBackend().failWith(
        SessionService.method.listSessions,
        Code.PermissionDenied,
        "not allowed to list sessions",
      ),
    );

    // Then — the failure is shown where the stack-parent picker would be
    errors
      .createSessionSessionsError()
      .should("be.visible")
      .and("have.text", "Sessions unavailable: Permission denied — not allowed to list sessions");
  });

  it("shows why the branch picker is empty when ListProjectBranches fails", () => {
    // Given — the only project is pre-selected, and its branches cannot be listed
    mountCreatePane(
      aCreateSessionBackend().failWith(
        ProjectService.method.listProjectBranches,
        Code.Unavailable,
        "git fetch failed",
      ),
    );
    createSessionPage.awaitProjectOption(ONLY_PROJECT.projectId);

    // When — the operator chooses to work on an existing branch
    createSessionPage.switchToWorkOnExistingBranch();

    // Then — the picker says why it has nothing to offer
    errors
      .createSessionBranchesError()
      .should("be.visible")
      .and("have.text", "Branches unavailable: git fetch failed");
  });

  it("switching project clears the previous project's branch failure", () => {
    // Given — two projects; the first one's branches cannot be listed, the second one's can
    const OTHER_PROJECT = { ...ONLY_PROJECT, projectId: "proj-other", name: "other" };
    mountCreatePane(
      aCreateSessionBackend()
        .onUnary(ProjectService.method.listProjects, () => ({
          projects: [ONLY_PROJECT, OTHER_PROJECT],
        }))
        .onUnary(ProjectService.method.listProjectBranches, (req) => {
          if (req.projectId === ONLY_PROJECT.projectId) {
            throw new ConnectError("git fetch failed", Code.Unavailable);
          }
          return { branches: ["origin/master"], defaultRemote: "origin" };
        }),
    );
    createSessionPage.awaitProjectOption(OTHER_PROJECT.projectId);
    createSessionPage.switchToWorkOnExistingBranch();
    createSessionPage.selectProject(ONLY_PROJECT.projectId);
    errors.createSessionBranchesError().should("be.visible");

    // When — the operator picks the project whose branches do load
    createSessionPage.selectProject(OTHER_PROJECT.projectId);

    // Then — the stale failure is gone
    errors.expectNoCreateSessionBranchesError();
  });
});

// ---------------------------------------------------------------------------
// Sessions drawer: project registry and notification feed
// ---------------------------------------------------------------------------

describe("SessionsDrawerScreen — the registry and the notification feed say when they fail", () => {
  it("shows why the project registry could not be read", () => {
    // Given — the drawer's ListProjects is refused
    const backend = aSessionServiceBackend({ sessions: [DRAWER_SESSION] }).failWith(
      ProjectService.method.listProjects,
      Code.Unavailable,
      "project registry is offline",
    );

    // When
    mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);

    // Then — the drawer says so instead of carrying on with an empty registry
    errors
      .drawerProjectsError()
      .should("be.visible")
      .and("have.text", "Projects unavailable: project registry is offline");
  });

  it("names the host that is not connected when no daemon client exists", () => {
    // Given — no daemon is advertised, so there is no client to read the registry through
    const backend = aSessionServiceBackend({ sessions: [] });

    // When
    mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />, []), backend);

    // Then — a visible "not connected", not a silently empty registry
    errors.drawerProjectsError().should("be.visible").and("contain.text", "Not connected to");
  });

  it("says the notification feed dropped, and keeps the dots it already had", () => {
    // Given — a drawer whose feed has reported the session waiting on the operator
    const notifications = aSessionNotificationFeed();
    const backend = aSessionServiceBackend({
      sessions: [DRAWER_SESSION],
      sessionNotifications: notifications,
    });
    mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
    cy.wrap(null, { log: false }).should(() => {
      expect(notifications.subscriptionCount()).to.equal(1);
    });
    cy.then(() => {
      notifications.push(anAttentionNotification(DRAWER_SESSION.sessionId, DRAWER_SESSION_LABEL));
    });
    sessionsDrawerPage.expectIndicator(DRAWER_SESSION.sessionId, "needs-input");

    // When — the daemon's feed drops
    cy.then(() => notifications.fail("notification feed closed"));

    // Then — the drawer says its dots are no longer live, and the last known state stays
    errors
      .drawerNotificationsError()
      .should("be.visible")
      .and("have.text", "Session notifications unavailable: notification feed closed");
    sessionsDrawerPage.expectIndicator(DRAWER_SESSION.sessionId, "needs-input");
  });
});

// ---------------------------------------------------------------------------
// Usage tab and activity stream
// ---------------------------------------------------------------------------

describe("Session streams — a dropped stream is reported beside what it fed", () => {
  it("keeps the last usage snapshot and says the usage stream dropped", () => {
    // Given — the usage stream delivers one snapshot and then drops
    const claude = aConversationRecord({ id: "claude-main", totalTokens: 15550n, turns: 7 });
    const backend = aUsageStreamThatDrops([[claude]], "presenter went away");

    // When
    mountWithRpc(<SessionUsageTab room={null} serverIdentity="presenter" />, backend);

    // Then — the table stays and the failure is explained beside it
    sessionsDrawerPage.usageRow("claude-main").should("be.visible");
    errors
      .usageStreamError()
      .should("be.visible")
      .and("have.text", "Usage unavailable: presenter went away");
  });

  it("says why there is no usage instead of claiming none was reported", () => {
    // Given — the usage stream drops before any snapshot
    const backend = aUsageStreamThatDrops([], "presenter went away");

    // When
    mountWithRpc(<SessionUsageTab room={null} serverIdentity="presenter" />, backend);

    // Then — the empty state is replaced by the reason
    errors.usageStreamError().should("have.text", "Usage unavailable: presenter went away");
    sessionsDrawerPage.usageEmpty().should("not.exist");
  });

  it("reports no failure for a session with no room, where no usage stream is possible", () => {
    // Given — a host reached without LiveKit: no room and no transport override

    // When
    cy.mount(<SessionUsageTab room={null} serverIdentity="presenter" />);

    // Then — the tab shows its empty state; nothing failed
    sessionsDrawerPage.usageEmpty().should("be.visible");
    errors.usageStreamError().should("not.exist");
  });

  it("reports no failure while the session's room is still joining", () => {
    // Given — a room that has not connected yet, and no transport override

    // When
    cy.mount(<SessionUsageTab room={new Room()} serverIdentity="presenter" />);

    // Then — the tab waits in its empty state rather than claiming the stream failed
    sessionsDrawerPage.usageEmpty().should("be.visible");
    errors.usageStreamError().should("not.exist");
  });

  it("reports a dropped activity stream and keeps the records it delivered", () => {
    // Given — the activity stream delivers one tool call and then drops
    const backend = anInMemoryRpcBackend().implement(ActivityService, {
      streamSessionActivity: async function* () {
        yield create(AgentActivityRecordSchema, { callId: "call-1" });
        throw new ConnectError("activity log unreadable", Code.Internal);
      },
    });
    const client = createClient(ActivityService, backend.transport());

    // When
    cy.mount(<ActivityStreamProbe client={client} />);

    // Then
    errors
      .activityStreamError()
      .should("be.visible")
      .and("have.text", "Activity unavailable: activity log unreadable");
    errors.expectActivityRecordsShown(1);
  });
});
