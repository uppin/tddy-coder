import React from "react";
import { createClient } from "@connectrpc/connect";
import { ConnectionService } from "../../src/gen/connection_pb";
import { SessionWorktreeTab } from "../../src/components/sessions/SessionWorktreeTab";
import { WORKTREE_STATS_REFRESH_MS } from "../../src/rpc/useSessionWorktreeStats";
import {
  aConnectionServiceBackend,
  type ConnectionServiceBackend,
} from "../support/rpc/connectionServiceBackend";
import { sessionWorktreeTabPage as page } from "../support/pages/sessionWorktreeTabPage";

const PROJECT_ID = "proj-worktree-inspector";
const SESSION_ID = "sess-worktree-1";
const REPO_PATH = "/repos/demo/.worktrees/feat-x";

// 1.2 GB expressed in bytes (1.2 * 1024^3) — formats to the label "1.2 GB".
const ONE_POINT_TWO_GB = 1288490189n;

function mountTab(backend: ConnectionServiceBackend, repoPath: string = REPO_PATH) {
  const client = createClient(ConnectionService, backend.transport());
  cy.mountWithRpc(
    <SessionWorktreeTab
      client={client}
      sessionToken="fake-token"
      projectId={PROJECT_ID}
      sessionId={SESSION_ID}
      repoPath={repoPath}
    />,
    backend,
  );
}

describe("Session Inspector — Worktree tab", () => {
  it("shows the session's worktree disk usage and branch", () => {
    // Given a project whose cached stats include the session's worktree
    const backend = aConnectionServiceBackend({
      worktrees: [
        {
          path: REPO_PATH,
          branchLabel: "feature/x",
          diskBytes: ONE_POINT_TWO_GB,
          changedFiles: 7,
          linesAdded: 240n,
          linesRemoved: 18n,
        },
      ],
    });

    // When the Worktree tab is shown
    mountTab(backend);

    // Then it renders that worktree's size and branch
    page.size().should("have.text", "1.2 GB");
    page.branch().should("have.text", "feature/x");
  });

  it("refreshes stats on a 10-minute cadence", () => {
    // Given the tab is open on a cached worktree
    cy.clock();
    const backend = aConnectionServiceBackend({
      worktrees: [{ path: REPO_PATH, branchLabel: "feature/x", diskBytes: ONE_POINT_TWO_GB }],
    });
    mountTab(backend);

    // Wait for the initial cached render so the mount effect (which registers the 10-minute
    // timer) has committed before the clock is advanced.
    page.size().should("have.text", "1.2 GB");

    // (mount used the cache-only path — no refresh yet)
    cy.wrap(null).should(() => {
      expect(backend.listWorktreesRefreshCount()).to.equal(0);
    });

    // When ten minutes pass
    cy.tick(WORKTREE_STATS_REFRESH_MS);

    // Then exactly one refreshing list call was made
    cy.wrap(null).should(() => {
      expect(backend.listWorktreesRefreshCount()).to.equal(1);
    });
  });

  it("clears the worktree only after the confirm step", () => {
    // Given the tab is open on a cached worktree
    const backend = aConnectionServiceBackend({
      worktrees: [{ path: REPO_PATH, branchLabel: "feature/x", diskBytes: ONE_POINT_TWO_GB }],
    });
    mountTab(backend);

    // When Clear is pressed once (first step only)
    page.clear().click();

    // Then nothing is cleared yet
    cy.wrap(null).should(() => {
      expect(backend.cleanedWorktreePaths).to.deep.equal([]);
    });

    // When the confirm step is pressed
    page.confirmClear().click();

    // Then the session's worktree is cleared
    cy.wrap(null).should(() => {
      expect(backend.cleanedWorktreePaths).to.deep.equal([REPO_PATH]);
    });
  });

  it("deletes the worktree only after the confirm step", () => {
    // Given the tab is open on a cached worktree
    const backend = aConnectionServiceBackend({
      worktrees: [{ path: REPO_PATH, branchLabel: "feature/x", diskBytes: ONE_POINT_TWO_GB }],
    });
    mountTab(backend);

    // When Delete is pressed once (first step only)
    page.delete().click();

    // Then nothing is removed yet
    cy.wrap(null).should(() => {
      expect(backend.removedWorktreePaths).to.deep.equal([]);
    });

    // When the confirm step is pressed
    page.confirmDelete().click();

    // Then the session's worktree is removed
    cy.wrap(null).should(() => {
      expect(backend.removedWorktreePaths).to.deep.equal([REPO_PATH]);
    });
  });

  it("offers Restore when the worktree is missing", () => {
    // Given the project has no worktree matching the session's repo path
    const backend = aConnectionServiceBackend({ worktrees: [] });

    // When the Worktree tab is shown
    mountTab(backend);

    // Then the missing state and Restore action are shown (Clear/Delete are not)
    page.missing().should("be.visible");
    page.clear().should("not.exist");
    page.delete().should("not.exist");

    // When Restore is pressed
    page.restore().click();

    // Then the session's worktree is restored from its persisted changeset
    cy.wrap(null).should(() => {
      expect(backend.restoredSessionIds).to.deep.equal([SESSION_ID]);
    });
  });
});
