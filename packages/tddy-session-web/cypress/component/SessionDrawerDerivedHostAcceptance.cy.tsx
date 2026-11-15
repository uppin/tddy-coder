/**
 * Acceptance: a session whose agent runs on a **derived host** — a machine its daemon reaches
 * through a remote shell, an SSH config alias — names that host on its drawer row.
 *
 * Such a session is owned by the daemon that reaches the host, so its `daemonInstanceId` is that
 * daemon's and says nothing about where the agent actually is. Where it runs is carried on
 * `SessionEntry.target`, and the row reads it from there.
 *
 * Mounted through `SessionDrawer` rather than `SessionDrawerItem`, as
 * `SessionsSplitPlacementAcceptance` is, so the component derives the badge from the entry:
 * passing a label in as a prop and asserting on it would compare the fixture against itself.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-15-remote-shell-derived-hosts.md.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { SessionEntrySchema, type SessionEntry } from "tddy-client-web/gen/session_pb";
import { SessionDrawer } from "../../src/SessionDrawer";
import { TooltipProvider } from "tddy-ui-web";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/** The daemon that owns both sessions, and the host selected in the drawer. */
const DAEMON = "laptop-a";
/** The SSH config alias of the machine that daemon reaches. */
const DERIVED_HOST = "buildbox";

const HOST_LABELS: Record<string, string> = {
  [DAEMON]: "laptop-a (this daemon)",
};

const SESSION_ON_BUILDBOX = "aaaaaaaa-0000-4000-8000-00000000000a";
const SESSION_ON_THE_DAEMON = "bbbbbbbb-0000-4000-8000-00000000000b";

/** A session owned by `laptop-a` whose agent runs on `buildbox`, in a worktree there. */
function aSessionOnTheDerivedHost(): SessionEntry {
  return create(SessionEntrySchema, {
    sessionId: SESSION_ON_BUILDBOX,
    createdAt: "2026-11-15T12:00:00Z",
    status: "active",
    repoPath: "/home/dev/repo/.worktrees/on-buildbox",
    pid: 41001,
    isActive: true,
    projectId: "proj-1",
    sessionType: "claude-cli",
    daemonInstanceId: DAEMON,
    pendingElicitation: false,
    target: { kind: { case: "ssh", value: { configHost: DERIVED_HOST } } },
  });
}

/** An ordinary session: its agent runs on the daemon's own host. */
function aSessionOnTheDaemon(): SessionEntry {
  return create(SessionEntrySchema, {
    sessionId: SESSION_ON_THE_DAEMON,
    createdAt: "2026-11-15T12:00:00Z",
    status: "active",
    repoPath: "/home/dev/repo/.worktrees/feature",
    pid: 41002,
    isActive: true,
    projectId: "proj-1",
    sessionType: "claude-cli",
    daemonInstanceId: DAEMON,
    pendingElicitation: false,
  });
}

/** The drawer, with the owning daemon selected — so neither row carries an owning-host badge. */
function mountDrawerWith(sessions: SessionEntry[]) {
  cy.mount(
    <TooltipProvider delayDuration={0}>
      <SessionDrawer
        sessions={sessions as Parameters<typeof SessionDrawer>[0]["sessions"]}
        selectedSessionId={null}
        onSelectSession={cy.stub()}
        isOpen
        onClose={cy.stub()}
        onOpen={cy.stub()}
        crossHostSessionsVisible
        selectedInstanceId={DAEMON}
        hostLabelForInstance={(instanceId: string) => HOST_LABELS[instanceId] ?? instanceId}
      />
    </TooltipProvider>,
  );
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800);
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

it("names the derived host a session runs on", () => {
  // Given a session on buildbox beside an ordinary one, both owned by the selected daemon
  mountDrawerWith([aSessionOnTheDerivedHost(), aSessionOnTheDaemon()]);

  // Then the row of the session on buildbox names buildbox
  sessionsDrawerPage.derivedHostBadge(SESSION_ON_BUILDBOX).should("have.text", DERIVED_HOST);
  // And the ordinary row names no derived host: an unconditional badge would place every session
  // somewhere it is not
  sessionsDrawerPage.drawerItem(SESSION_ON_THE_DAEMON).should("exist");
  sessionsDrawerPage.expectNoDerivedHostBadge(SESSION_ON_THE_DAEMON);
});
