/**
 * The Code pane's bottom strip shows where the session's code index stands — the phase running now,
 * its percentage and the index server's own line while it loads, "ready" once it is queryable, and
 * why when the warm failed or nothing warmed it.
 *
 * Feature: docs/ft/web/session-code-pane.md (Indexing indicator).
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { ConnectError, Code, createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CodeIndexProgressSchema, CodeNavigationService } from "../../src/gen/code_navigation_pb";
import { WorktreeService } from "../../src/gen/worktree_pb";
import { WorktreeCodePane } from "../../src/components/session/WorktreeCodePane";
import { mountWithRpc } from "../support/rpc/inMemory";
import { worktreeCodePanePage } from "../support/pages/worktreeCodePanePage";

const THE_SESSION = "session-indexing-strip";

type Gate = { held: Promise<void>; release: () => void };

function aGate(): Gate {
  let release: () => void = () => undefined;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  return { held, release: () => release() };
}

/** A host whose warm reports `progress` messages, waits for `gate`, then ends with `last`. */
function aHostWarming(
  progress: Parameters<typeof create<typeof CodeIndexProgressSchema>>[1][],
  gate: Gate,
  last: Parameters<typeof create<typeof CodeIndexProgressSchema>>[1] | "fail",
): InMemoryRpcBackend {
  return anInMemoryRpcBackend().implement(CodeNavigationService, {
    async *watchCodeIndex() {
      for (const message of progress) yield create(CodeIndexProgressSchema, message);
      await gate.held;
      if (last === "fail") throw new ConnectError("index daemon unreachable", Code.Unavailable);
      yield create(CodeIndexProgressSchema, last);
    },
  });
}

/** A host that warms nothing for the session: the watch ends without a message. */
function aHostWarmingNothing(): InMemoryRpcBackend {
  return anInMemoryRpcBackend().implement(CodeNavigationService, {
    async *watchCodeIndex() {},
  });
}

function mountTheCodePane(backend: InMemoryRpcBackend) {
  mountWithRpc(
    <div style={{ height: 480, width: 900, display: "flex" }}>
      <WorktreeCodePane
        client={createClient(WorktreeService, backend.transport())}
        sessionToken="fake-token"
        projectId="proj-1"
        worktreePath="/home/dev/repo/.worktrees/feature"
        navigationClient={createClient(CodeNavigationService, backend.transport())}
        sessionId={THE_SESSION}
      />
    </div>,
    backend,
  );
}

beforeEach(() => {
  cy.viewport(1280, 900);
});

it("shows the phase running now and what the server is doing, then that the index is ready", () => {
  // Given a host whose warm finished scanning roots and is now indexing tddy_core
  const gate = aGate();
  mountTheCodePane(
    aHostWarming(
      [
        {
          line: "Indexing: 120/310 (tddy_core)",
          phase: "Indexing",
          percentage: 38,
          furthest: "Indexing: 120/310 (tddy_core); furthest Roots Scanned 100%",
        },
      ],
      gate,
      { line: "ready", ready: true },
    ),
  );

  // Then the strip names the phase, its percentage and the server's line
  worktreeCodePanePage.indexStatus().should("have.attr", "data-state", "progress");
  worktreeCodePanePage.indexStatusSummary().should("have.text", "Indexing — Indexing 38%");
  worktreeCodePanePage.indexStatusDetail().should("have.text", "Indexing: 120/310 (tddy_core)");

  // When the index becomes ready
  cy.then(() => gate.release());

  // Then the strip stays, saying so
  worktreeCodePanePage.indexStatus().should("have.attr", "data-state", "ready");
  worktreeCodePanePage.indexStatusSummary().should("have.text", "Code index ready");
  worktreeCodePanePage.indexStatusDetail().should("not.exist");
});

it("says the index is not warmed when nothing on the host warms the session", () => {
  // Given a host that warms nothing for the session
  mountTheCodePane(aHostWarmingNothing());

  // Then the strip says the index loads on first use
  worktreeCodePanePage.indexStatus().should("have.attr", "data-state", "untracked");
  worktreeCodePanePage
    .indexStatusSummary()
    .should("have.text", "Code index not warmed for this session");
});

it("names the reason when following the warm fails", () => {
  // Given a host whose warm watch fails partway
  const gate = aGate();
  mountTheCodePane(
    aHostWarming([{ line: "Fetching", phase: "Fetching", percentage: 10 }], gate, "fail"),
  );
  worktreeCodePanePage.indexStatus().should("have.attr", "data-state", "progress");

  // When the watch fails
  cy.then(() => gate.release());

  // Then the strip names the failure
  worktreeCodePanePage.indexStatus().should("have.attr", "data-state", "failed");
  worktreeCodePanePage
    .indexStatusSummary()
    .should("contain.text", "Indexing failed")
    .and("contain.text", "index daemon unreachable");
});
