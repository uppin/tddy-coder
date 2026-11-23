/**
 * Cypress component acceptance: a target session's tmux tabs open on its target — attaching to one
 * of the target's tmux sessions and "New tmux session…" open wrapped in `target`, and an agent "in
 * tmux" opens wrapped in `[target, tmux]` (`#target-tmux` 2/2). A session on this host opens them
 * unwrapped, as before (`NewTerminalMenuTmuxAcceptance`, `SessionTerminalTabsTmuxAcceptance`).
 *
 * PRD: `docs/ft/daemon/1-WIP/PRD-2026-11-23-target-tmux.md` (n2)
 *
 * Driven over the deterministic gRPC path: `connectSession` returns an empty `livekitRoom`, so every
 * terminal RPC flows over the daemon client into the in-memory backend, which records each request.
 */

import React from "react";
import type { SessionEntry } from "tddy-client-web/gen/session_pb";
import { TerminalSessionService } from "tddy-client-web/gen/terminal_session_pb";
import { SessionsDrawerScreen } from "../../src/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import {
  aSessionServiceBackend,
  type SessionServiceBackend,
} from "../support/rpc/daemonSessionHostBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { sessionTerminalTabsPage as tabs } from "../support/pages/sessionTerminalTabsPage";

// ---------------------------------------------------------------------------
// Fixture — one active claude-cli session on an ssh target, attached over gRPC
// ---------------------------------------------------------------------------

const SESSION_ID = "target-tmux-aaaaaaaa-0000-0000-0000-000000000001";

/** A session whose agent runs on the ssh target `buildbox`. */
function aTargetSession(): Partial<SessionEntry> {
  return {
    sessionId: SESSION_ID,
    createdAt: "2026-11-23T09:00:00Z",
    status: "active",
    repoPath: `/home/dev/repo/.worktrees/${SESSION_ID}`,
    pid: 90021,
    isActive: true,
    projectId: "proj-target-tmux",
    daemonInstanceId: "local",
    pendingElicitation: false,
    sessionType: "claude-cli",
    target: { kind: { case: "ssh", value: { configHost: "buildbox" } } },
  } as Partial<SessionEntry>;
}

/** A target session whose target keeps the tmux session `work`, and whose daemon runs Muse Code;
 *  started terminals are `term-1`, `term-2`, … */
function aBackendServingATargetSession(): SessionServiceBackend {
  return aSessionServiceBackend({
    sessions: [aTargetSession()],
    agents: [{ id: "muse", label: "Muse Code" }],
    tmuxSessions: [{ name: "work" }],
    connectSession: () => ({ livekitRoom: "", livekitUrl: "", livekitServerIdentity: "" }),
    newTerminalId: (index) => `term-${index + 1}`,
  });
}

function attachSession(backend: SessionServiceBackend) {
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(SESSION_ID).click();
  tabs.tabs().should("exist");
}

/** The kind, tmux session and wrapper kinds of every `StartTerminalSession` the backend received,
 *  in call order. */
function startedTerminals(backend: SessionServiceBackend) {
  return backend
    .callsTo(TerminalSessionService.method.startTerminalSession)
    .map((request) => ({
      kind: request.kind,
      tmuxSession: request.tmuxSession,
      wrappers: request.wrappers.map((wrapper) => wrapper.kind),
    }));
}

// ---------------------------------------------------------------------------

describe("NewTerminalMenu — tmux tabs on a target", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("a target session's tmux items open on the target", () => {
    // Given a connected session on a target that keeps the tmux session "work"
    const backend = aBackendServingATargetSession();
    attachSession(backend);

    // When a tab is attached to "work", then a new tmux session "scratch" is opened
    tabs.newTab().click();
    tabs.tmuxOption("work").click();
    tabs.newTab().click();
    tabs.tmuxNewOption().click();
    tabs.tmuxNameInput().clear().type("scratch{enter}");

    // Then the daemon is asked for both on the target
    cy.wrap(backend).should((b: SessionServiceBackend) => {
      expect(startedTerminals(b)).to.deep.equal([
        { kind: "tmux", tmuxSession: "work", wrappers: ["target"] },
        { kind: "tmux-new", tmuxSession: "scratch", wrappers: ["target"] },
      ]);
    });
  });

  it("a target session's agent in tmux opens in tmux on the target", () => {
    // Given a connected session on a target whose daemon runs Muse Code
    const backend = aBackendServingATargetSession();
    attachSession(backend);

    // When Muse Code is opened "in tmux"
    tabs.newTab().click();
    tabs.newTabInTmuxOption("muse-cli").click();

    // Then the daemon is asked for Muse Code in tmux on the target
    cy.wrap(backend).should((b: SessionServiceBackend) => {
      expect(startedTerminals(b)).to.deep.equal([
        { kind: "muse-cli", tmuxSession: "", wrappers: ["target", "tmux"] },
      ]);
    });
  });
});
