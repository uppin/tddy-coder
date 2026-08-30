/**
 * Acceptance: the Agent roster pane shows what each attached agent is *doing*, live.
 *
 * Feature: docs/ft/daemon/session-agent-roster.md § What an agent is doing.
 *
 * The roster pane already said what an agent *is* and whether its checkout was ready. It said
 * nothing about whether the agent was working, so an operator watching a session could not tell a
 * dispatched agent from an idle one, and the only way to find out was to prompt it.
 *
 * Two properties are under test here, and neither is "the pane renders a field":
 *
 * - **A status arrives at the revision already applied.** `rev` moves on an attach or a detach;
 *   what an agent is doing moves whenever it starts or finishes a turn, and the daemon republishes
 *   the *same* rev to say so. A pane that only re-rendered on a new `rev` would show the state each
 *   agent was in when it was attached, for the life of the session.
 * - **`UNSPECIFIED` is not `idle`.** The daemon sends it when it has nothing to say. An operator
 *   told an agent is idle reads "free, ready for work", which is a different claim from "nobody
 *   here knows".
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { SessionAgentStatus, SessionEntrySchema } from "../../src/gen/connection_pb";
import { SessionAgentRosterPane } from "../../src/components/sessions/SessionAgentRosterPane";
import type { DaemonHost } from "../../src/lib/participantRole";
import {
  aRemoteAttachedAgent,
  aSessionAgentRosterBackend,
  anActivity,
  anAgentDoing,
  anAttachedAgent,
  type RosterBackend,
} from "../support/rpc/sessionAgentRosterBackend";
import { sessionAgentRosterPage as page } from "../support/pages/sessionAgentRosterPage";

const SESSION_ID = "1780828020298-roster";
const EXPLORER = "explorer@workstation-1";
const REVIEWER = "reviewer@workstation-1";
const LINTER_REMOTE = "linter@server-2";

const HOST_A: DaemonHost = { instanceId: "workstation-1", label: "workstation-1 (this daemon)" };

/** The co-located session the pane is mounted for — its roster half is the session itself. */
const SESSION = create(SessionEntrySchema, {
  sessionId: SESSION_ID,
  createdAt: "2026-08-29T09:00:00Z",
  status: "active",
  repoPath: "/home/dev/project",
  pid: 90001,
  isActive: true,
  projectId: "proj-1",
  daemonInstanceId: HOST_A.instanceId,
  sessionType: "claude-cli",
  agent: "claude",
  model: "opus-4",
});

/** A fixed "now", so an age in the rendered text is a fact about the fixture, not about the clock. */
const NOW_MS = 1_780_828_020_298;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/**
 * Publish a revision in command-queue order — see the note in `SessionAgentRosterPane.cy.tsx`. A
 * bare call would run during test-body evaluation and be folded into the first frame, so it would
 * never be the *live* change these tests are about.
 */
function publish(
  roster: RosterBackend,
  agents: Parameters<typeof roster.pushRoster>[0],
  rev: number,
) {
  cy.then(() => roster.pushRoster(agents, rev));
}

function mountPane(roster: RosterBackend) {
  cy.mountWithRpc(
    <SessionAgentRosterPane
      session={SESSION}
      sessions={[SESSION]}
      sessionToken="tok"
      daemonConnected
      onSwitchSubagent={cy.stub().as("switchSubagent")}
    />,
    roster.backend,
  );
}

describe("Agent roster — what each agent is doing", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    // The relative age in `last_activity` is rendered against the wall clock; pinning it keeps
    // "just now" from becoming "1s ago" on a slow machine.
    cy.clock(NOW_MS);
  });

  it("shows an agent nothing has been observed of as unknown, not as idle", () => {
    // Given the daemon has nothing to say about the agent — a roster restored after a restart
    const roster = aSessionAgentRosterBackend({
      sessionId: SESSION_ID,
      rev: 1,
      initial: [anAttachedAgent(EXPLORER)],
    });

    // When
    mountPane(roster);

    // Then — "idle" would read as "free, ready for work", which is a claim nobody has made
    page.assertStatus(EXPLORER, "unknown");
    page.rowStatus(EXPLORER).should("have.text", "unknown");
  });

  it("shows no last-activity line for an agent nothing has been observed of", () => {
    // An empty line reserved for an agent with no history is a row that looks like it lost one.
    const roster = aSessionAgentRosterBackend({
      sessionId: SESSION_ID,
      rev: 1,
      initial: [anAttachedAgent(EXPLORER)],
    });

    mountPane(roster);

    page.row(EXPLORER).should("exist");
    page.rowLastActivity(EXPLORER).should("not.exist");
  });

  it("shows what the agent is doing and what it was last seen doing", () => {
    // Given
    const roster = aSessionAgentRosterBackend({
      sessionId: SESSION_ID,
      rev: 1,
      initial: [
        anAgentDoing(
          EXPLORER,
          SessionAgentStatus.RUNNING,
          anActivity("prompted: find the caller", NOW_MS - 4 * 60_000),
        ),
      ],
    });

    // When
    mountPane(roster);

    // Then
    page.assertStatus(EXPLORER, "running");
    page
      .rowLastActivity(EXPLORER)
      .should("contain.text", "prompted: find the caller")
      .and("contain.text", "4m ago");
  });

  it("follows a status change published at the revision already applied", () => {
    // Given an attached agent doing nothing in particular
    const roster = aSessionAgentRosterBackend({
      sessionId: SESSION_ID,
      rev: 1,
      initial: [anAttachedAgent(EXPLORER)],
    });
    mountPane(roster);
    page.assertStatus(EXPLORER, "unknown");

    // When the agent starts a turn — the roster itself did not change, so `rev` does not move
    publish(
      roster,
      [
        anAgentDoing(
          EXPLORER,
          SessionAgentStatus.RUNNING,
          anActivity("prompted: summarise the diff", NOW_MS),
        ),
      ],
      1,
    );

    // Then — a pane that re-rendered only on a new `rev` would still say "unknown"
    page.assertStatus(EXPLORER, "running");
    page.rowLastActivity(EXPLORER).should("contain.text", "prompted: summarise the diff");
  });

  it("follows the agent through a turn and back to idle", () => {
    // Given
    const roster = aSessionAgentRosterBackend({
      sessionId: SESSION_ID,
      rev: 1,
      initial: [anAttachedAgent(EXPLORER)],
    });
    mountPane(roster);

    // When the agent is prompted, enters a tool call, and finishes — all at one revision
    publish(
      roster,
      [anAgentDoing(EXPLORER, SessionAgentStatus.RUNNING, anActivity("prompted: x", NOW_MS))],
      1,
    );
    page.assertStatus(EXPLORER, "running");

    publish(
      roster,
      [
        anAgentDoing(
          EXPLORER,
          SessionAgentStatus.EXECUTING_TOOL,
          anActivity("Read src/main.rs", NOW_MS),
        ),
      ],
      1,
    );
    page.assertStatus(EXPLORER, "executing-tool");
    page.rowLastActivity(EXPLORER).should("contain.text", "Read src/main.rs");

    publish(
      roster,
      [
        anAgentDoing(
          EXPLORER,
          SessionAgentStatus.IDLE,
          anActivity("answered (412 chars)", NOW_MS),
        ),
      ],
      1,
    );

    // Then
    page.assertStatus(EXPLORER, "idle");
    page.rowLastActivity(EXPLORER).should("contain.text", "answered (412 chars)");
  });

  it("keeps one agent's turn off another agent's row", () => {
    // Given two attached agents and a turn in flight with only one of them
    const roster = aSessionAgentRosterBackend({
      sessionId: SESSION_ID,
      rev: 1,
      initial: [
        anAgentDoing(EXPLORER, SessionAgentStatus.RUNNING, anActivity("prompted", NOW_MS)),
        anAttachedAgent(REVIEWER),
      ],
    });

    // When
    mountPane(roster);

    // Then
    page.assertStatus(EXPLORER, "running");
    page.assertStatus(REVIEWER, "unknown");
    page.rowLastActivity(REVIEWER).should("not.exist");
  });

  it("shows an agent whose checkout is still being built as connecting", () => {
    // Given a remote agent mid-provision. The clone outranks the conversation: an agent whose
    // checkout is not ready refuses prompts, so the daemon sends CONNECTING however idle its
    // conversation looks — and the pane must not second-guess that from `cloneState`.
    const roster = aSessionAgentRosterBackend({
      sessionId: SESSION_ID,
      rev: 1,
      initial: [
        aRemoteAttachedAgent(LINTER_REMOTE, {
          status: SessionAgentStatus.CONNECTING,
        }),
      ],
    });

    // When
    mountPane(roster);

    // Then
    page.assertStatus(LINTER_REMOTE, "connecting");
  });

  it("shows an agent whose checkout failed as error", () => {
    const roster = aSessionAgentRosterBackend({
      sessionId: SESSION_ID,
      rev: 1,
      initial: [
        aRemoteAttachedAgent(LINTER_REMOTE, {
          status: SessionAgentStatus.ERROR,
          cloneError: "git clone failed: repository not found",
        }),
      ],
    });

    mountPane(roster);

    page.assertStatus(LINTER_REMOTE, "error");
  });

  it("shows an agent blocked on a human as waiting for input", () => {
    const roster = aSessionAgentRosterBackend({
      sessionId: SESSION_ID,
      rev: 1,
      initial: [
        anAgentDoing(
          EXPLORER,
          SessionAgentStatus.WAITING_FOR_INPUT,
          anActivity("needs approval to run: rm -rf build/", NOW_MS),
        ),
      ],
    });

    mountPane(roster);

    page.assertStatus(EXPLORER, "waiting-for-input");
    page.rowStatus(EXPLORER).should("have.text", "waiting for input");
  });

  it("ages the last-activity line without a new roster frame", () => {
    // Given an agent whose last activity was just now
    const roster = aSessionAgentRosterBackend({
      sessionId: SESSION_ID,
      rev: 1,
      initial: [
        anAgentDoing(EXPLORER, SessionAgentStatus.IDLE, anActivity("answered", NOW_MS)),
      ],
    });
    mountPane(roster);
    page.rowLastActivity(EXPLORER).should("contain.text", "just now");

    // When time passes and nothing is republished — an idle agent produces no frames at all, so a
    // line that only aged on a frame would read "just now" for the rest of the session
    cy.tick(5 * 60_000);

    // Then
    page.rowLastActivity(EXPLORER).should("contain.text", "5m ago");
  });
});
