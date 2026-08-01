/**
 * Acceptance tests: a planned PR that is behind its base offers to pull the base in.
 *
 * When a predecessor lands commits, the operator previously had to ask the agent in chat or open a
 * terminal in the worktree. Repoint is the wrong tool — it answers "this node belongs somewhere
 * else now" by dropping parent edges, whereas the operator wants to stay stacked exactly where they
 * are and take what the base has.
 *
 * Merge is the default: it adds a merge commit, rewrites no history, and disturbs no review anchors
 * on the open PR — the same operation the stack's own `pr_resolve_conflicts` performs. Rebase is
 * offered beside it as an explicit choice, because it rewrites history and force-pushes.
 *
 * The control is offered **only** when the pull is meaningful and safe to name: not at zero commits,
 * not on conflicts, and never on a comparison that could not be made. An action derived from an
 * unavailable comparison would be an action derived from nothing.
 *
 * PRD: docs/ft/coder/1-WIP/PRD-2026-08-01-pr-stack-panel-ux.md (C5, D30–D33; AC 16–21).
 */

import React from "react";
import { Code, ConnectError } from "@connectrpc/connect";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { ConnectionService, type ProjectEntry, type SessionEntry } from "../../src/gen/connection_pb";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";
import {
  aPlannedNode,
  aStackPlanJson,
  aBranchResolutionResponse,
  aPullBaseIntoBranchResponse,
  type BranchResolutionFixture,
  type StackNodeFixture,
} from "../support/rpc/prStackFixtures";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const ORCHESTRATOR_SESSION_ID = "pr-stack-session-7500-0000-0000-0000-000000000075";
const PROJECT_ID = "proj-pr-stack";
const DEFAULT_BRANCH = "origin/master";

const ROOT_BRANCH = "feature/auth/token-store";
const CHILD_BRANCH = "feature/auth/middleware";
const CHILD_WORKTREE_PATH = "/home/dev/worktrees/middleware";

function aTwoNodeStack(): StackNodeFixture[] {
  return [
    aPlannedNode({ nodeId: "n1", title: "Add token store", branch: ROOT_BRANCH }),
    aPlannedNode({
      nodeId: "n2",
      title: "Add auth middleware",
      branch: CHILD_BRANCH,
      parents: ["n1"],
    }),
  ];
}

function anOrchestratorSession(stackPlanJson: string): Partial<SessionEntry> {
  return {
    sessionId: ORCHESTRATOR_SESSION_ID,
    createdAt: "2026-08-01T09:00:00Z",
    status: "idle",
    repoPath: "/home/dev/pr-stack-project",
    isActive: false,
    projectId: PROJECT_ID,
    recipe: "pr-stack",
    stackPlanJson,
  };
}

const PROJECT: Partial<ProjectEntry> = {
  projectId: PROJECT_ID,
  name: "pr-stack-project",
  gitUrl: "https://example.com/pr-stack.git",
  mainRepoPath: "/home/dev/pr-stack-project",
  mainBranchRef: DEFAULT_BRANCH,
  daemonInstanceId: "local",
};

/** A resolution reporting the child branch `behind` commits behind its base, cleanly. */
function behindBy(behind: number, worktree?: { dirty: boolean; dirtyPaths?: string[] }): BranchResolutionFixture {
  return {
    branch: CHILD_BRANCH,
    worktree: {
      exists: true,
      path: CHILD_WORKTREE_PATH,
      dirty: worktree?.dirty ?? false,
      dirtyPaths: worktree?.dirtyPaths ?? [],
    },
    baseSync: { baseBranch: ROOT_BRANCH, behindCount: behind, aheadCount: 1 },
  };
}

interface MountOptions {
  nodes?: StackNodeFixture[];
  resolutionByBranch?: Record<string, BranchResolutionFixture>;
  /** The resolution the daemon returns after a successful pull. */
  afterPull?: BranchResolutionFixture;
}

function aPrStackBackend(opts: MountOptions) {
  return aSessionsDrawerBackend([
    anOrchestratorSession(aStackPlanJson(1, opts.nodes ?? aTwoNodeStack())),
  ])
    .onUnary(ConnectionService.method.queryBranch, (req: { branch: string }) =>
      aBranchResolutionResponse(
        opts.resolutionByBranch?.[req.branch] ?? { branch: req.branch },
      ),
    )
    .onUnary(ConnectionService.method.listProjects, () => ({ projects: [PROJECT] }))
    .onUnary(ConnectionService.method.listTools, () => ({ tools: [] }));
}

function mountAndOpen(backend: ReturnType<typeof aPrStackBackend>) {
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
  return backend;
}

/** The branch as it stands once the base has been pulled in and pushed. */
const PULLED_IN: BranchResolutionFixture = {
  branch: CHILD_BRANCH,
  baseSync: { baseBranch: ROOT_BRANCH, behindCount: 0, aheadCount: 4 },
};

/** Open the screen with a `PullBaseIntoBranch` that succeeds and returns a fresh resolution. */
function openPrStackScreen(opts: MountOptions) {
  return mountAndOpen(
    aPrStackBackend(opts).onUnary(ConnectionService.method.pullBaseIntoBranch, () =>
      aPullBaseIntoBranchResponse({ resolution: opts.afterPull ?? PULLED_IN }),
    ),
  );
}

/**
 * Open the screen with a `PullBaseIntoBranch` whose merge landed in the branch but whose push did
 * not — a *successful* call reporting `pushed = false` and the daemon's reason (D32).
 */
function openPrStackScreenWithUnpushedPull(pushError: string, opts: MountOptions) {
  return mountAndOpen(
    aPrStackBackend(opts).onUnary(ConnectionService.method.pullBaseIntoBranch, () =>
      aPullBaseIntoBranchResponse({ resolution: PULLED_IN, pushed: false, pushError }),
    ),
  );
}

/** Open the screen with a `PullBaseIntoBranch` the daemon refuses, carrying `message` as its reason. */
function openPrStackScreenWithRefusedPull(message: string, opts: MountOptions) {
  return mountAndOpen(
    aPrStackBackend(opts).onUnary(ConnectionService.method.pullBaseIntoBranch, () => {
      throw new ConnectError(message, Code.FailedPrecondition);
    }),
  );
}

/** Open the screen with a `PullBaseIntoBranch` that never answers — the call stays in flight. */
function openPrStackScreenWithPullInFlight(opts: MountOptions) {
  return mountAndOpen(
    aPrStackBackend(opts).onUnary(
      ConnectionService.method.pullBaseIntoBranch,
      () => new Promise<never>(() => undefined),
    ),
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
});

// ---------------------------------------------------------------------------
// The control and what it promises
// ---------------------------------------------------------------------------

it("offers to merge the commits a planned PR is missing from its base", () => {
  // Given
  openPrStackScreen({ resolutionByBranch: { [CHILD_BRANCH]: behindBy(3) } });

  // When
  prStackScreenPage.expandRow("n2");

  // Then — the operator knows how much is coming and from where before clicking
  prStackScreenPage
    .syncMergeBtn("n2")
    .should("contain.text", "3")
    .and("contain.text", ROOT_BRANCH);
});

it("names a single missing commit in the singular", () => {
  // Given
  openPrStackScreen({ resolutionByBranch: { [CHILD_BRANCH]: behindBy(1) } });

  // When
  prStackScreenPage.expandRow("n2");

  // Then
  prStackScreenPage.syncMergeBtn("n2").should("contain.text", "1 commit").and("not.contain.text", "1 commits");
});

it("offers rebasing onto the base beside merging it", () => {
  // Given
  openPrStackScreen({ resolutionByBranch: { [CHILD_BRANCH]: behindBy(3) } });

  // When
  prStackScreenPage.expandRow("n2");

  // Then
  prStackScreenPage.syncRebaseBtn("n2").should("contain.text", ROOT_BRANCH);
});

// ---------------------------------------------------------------------------
// Pulling
// ---------------------------------------------------------------------------

it("merges the base into the branch when the merge control is clicked", () => {
  // Given
  const backend = openPrStackScreen({ resolutionByBranch: { [CHILD_BRANCH]: behindBy(3) } });
  prStackScreenPage.expandRow("n2");

  // When
  prStackScreenPage.clickSyncMerge("n2");

  // Then — the daemon is asked for exactly the base the control named, by the default strategy
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ConnectionService.method.pullBaseIntoBranch);
    expect(calls).to.have.length(1);
    expect(calls[0].sessionId).to.equal(ORCHESTRATOR_SESSION_ID);
    expect(calls[0].nodeId).to.equal("n2");
    expect(calls[0].baseBranch).to.equal(ROOT_BRANCH);
    expect(calls[0].strategy).to.equal("merge");
  });
});

it("rebases the branch onto the base when the rebase control is clicked", () => {
  // Given
  const backend = openPrStackScreen({ resolutionByBranch: { [CHILD_BRANCH]: behindBy(3) } });
  prStackScreenPage.expandRow("n2");

  // When
  prStackScreenPage.clickSyncRebase("n2");

  // Then
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ConnectionService.method.pullBaseIntoBranch);
    expect(calls).to.have.length(1);
    expect(calls[0].strategy).to.equal("rebase");
  });
});

it("repaints the row from the pull's own result rather than waiting for the next poll", () => {
  // Given
  openPrStackScreen({ resolutionByBranch: { [CHILD_BRANCH]: behindBy(3) } });
  prStackScreenPage.expandRow("n2");

  // When
  prStackScreenPage.clickSyncMerge("n2");

  // Then
  prStackScreenPage.baseInSync("n2").should("exist");
  prStackScreenPage.syncMergeBtn("n2").should("not.exist");
});

it("says nothing about the remote when the pull reached it", () => {
  // Given
  openPrStackScreen({ resolutionByBranch: { [CHILD_BRANCH]: behindBy(3) } });
  prStackScreenPage.expandRow("n2");

  // When
  prStackScreenPage.clickSyncMerge("n2");

  // Then — a completed pull is reported by the row going in sync, and by nothing else
  prStackScreenPage.syncError("n2").should("not.exist");
});

it("disables both controls while a pull is in flight", () => {
  // Given — a merge and a rebase of one branch running side by side is destructive, not merely
  // wasteful, so neither control may start a second operation
  openPrStackScreenWithPullInFlight({ resolutionByBranch: { [CHILD_BRANCH]: behindBy(3) } });
  prStackScreenPage.expandRow("n2");

  // When
  prStackScreenPage.clickSyncMerge("n2");

  // Then
  prStackScreenPage.syncMergeBtn("n2").should("be.disabled");
  prStackScreenPage.syncRebaseBtn("n2").should("be.disabled");
});

// ---------------------------------------------------------------------------
// When the control is not offered
// ---------------------------------------------------------------------------

it("offers no pull when the branch is already in sync with its base", () => {
  // Given — a zero-commit merge still runs a git operation, and offering it invites a click that
  // can only surprise
  openPrStackScreen({ resolutionByBranch: { [CHILD_BRANCH]: behindBy(0) } });

  // When
  prStackScreenPage.expandRow("n2");

  // Then
  prStackScreenPage.syncMergeBtn("n2").should("not.exist");
  prStackScreenPage.syncRebaseBtn("n2").should("not.exist");
});

it("offers no pull when the branch conflicts with its base", () => {
  // Given
  openPrStackScreen({
    resolutionByBranch: {
      [CHILD_BRANCH]: {
        branch: CHILD_BRANCH,
        worktree: { exists: true, path: CHILD_WORKTREE_PATH },
        baseSync: {
          baseBranch: ROOT_BRANCH,
          behindCount: 4,
          hasConflicts: true,
          conflictedPaths: ["src/auth/mod.rs"],
        },
      },
    },
  });

  // When
  prStackScreenPage.expandRow("n2");

  // Then — the conflicted paths are what the row offers instead
  prStackScreenPage.syncMergeBtn("n2").should("not.exist");
  prStackScreenPage.baseConflictPaths("n2").should("exist");
});

it("offers no pull when the base comparison could not be made", () => {
  // Given
  openPrStackScreen({
    resolutionByBranch: {
      [CHILD_BRANCH]: {
        branch: CHILD_BRANCH,
        worktree: { exists: true, path: CHILD_WORKTREE_PATH },
        baseSync: {
          baseBranch: ROOT_BRANCH,
          unavailable: true,
          unavailableReason: "not a git repository",
        },
      },
    },
  });

  // When
  prStackScreenPage.expandRow("n2");

  // Then — an action derived from a comparison that was not made is an action derived from nothing
  prStackScreenPage.syncMergeBtn("n2").should("not.exist");
  prStackScreenPage.syncRebaseBtn("n2").should("not.exist");
});

it("offers no pull for a planned PR that owns no branch yet", () => {
  // Given — nothing to merge into
  const nodes = [
    aPlannedNode({ nodeId: "n1", title: "Add token store", branch: ROOT_BRANCH }),
    aPlannedNode({
      nodeId: "n2",
      title: "Add auth middleware",
      branchSuggestion: CHILD_BRANCH,
      parents: ["n1"],
    }),
  ];
  openPrStackScreen({ nodes });

  // When
  prStackScreenPage.expandRow("n2");

  // Then
  prStackScreenPage.syncMergeBtn("n2").should("not.exist");
});

// ---------------------------------------------------------------------------
// A worktree with uncommitted work
// ---------------------------------------------------------------------------

it("prompts before pulling into a worktree holding uncommitted changes", () => {
  // Given — a child session's agent may be mid-turn in this checkout
  openPrStackScreen({
    resolutionByBranch: {
      [CHILD_BRANCH]: behindBy(3, { dirty: true, dirtyPaths: ["src/auth/mod.rs"] }),
    },
  });
  prStackScreenPage.expandRow("n2");

  // When
  prStackScreenPage.clickSyncMerge("n2");

  // Then — nothing is touched until the operator has seen what is outstanding
  prStackScreenPage.dirtyWorktreeDialog().should("exist");
  prStackScreenPage.dirtyWorktreePaths().should("contain.text", "src/auth/mod.rs");
});

it("commits and pushes the outstanding work before pulling when the operator confirms", () => {
  // Given
  const backend = openPrStackScreen({
    resolutionByBranch: {
      [CHILD_BRANCH]: behindBy(3, { dirty: true, dirtyPaths: ["src/auth/mod.rs"] }),
    },
  });
  prStackScreenPage.expandRow("n2");
  prStackScreenPage.clickSyncMerge("n2");

  // When
  prStackScreenPage.commitDirtyWorktreeAndPull("wip: auth middleware");

  // Then
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ConnectionService.method.pullBaseIntoBranch);
    expect(calls).to.have.length(1);
    expect(calls[0].dirtyWorktreeAction).to.equal("commit");
    expect(calls[0].commitMessage).to.equal("wip: auth middleware");
    expect(calls[0].strategy).to.equal("merge");
  });
});

it("leaves the worktree alone when the operator cancels the prompt", () => {
  // Given
  const backend = openPrStackScreen({
    resolutionByBranch: {
      [CHILD_BRANCH]: behindBy(3, { dirty: true, dirtyPaths: ["src/auth/mod.rs"] }),
    },
  });
  prStackScreenPage.expandRow("n2");
  prStackScreenPage.clickSyncMerge("n2");

  // When
  prStackScreenPage.dirtyWorktreeCancelBtn().click();

  // Then
  prStackScreenPage.dirtyWorktreeDialog().should("not.exist");
  cy.wrap(backend).should((b) => {
    expect(b.callsTo(ConnectionService.method.pullBaseIntoBranch)).to.have.length(0);
  });
});

// ---------------------------------------------------------------------------
// A refused or failed pull
// ---------------------------------------------------------------------------

it("shows the daemon's reason inline when a pull fails", () => {
  // Given
  openPrStackScreenWithRefusedPull(
    "merging origin/feature/auth/token-store conflicts in src/auth/mod.rs",
    { resolutionByBranch: { [CHILD_BRANCH]: behindBy(3) } },
  );
  prStackScreenPage.expandRow("n2");

  // When
  prStackScreenPage.clickSyncMerge("n2");

  // Then — and the control comes back, because the operator may want to retry
  prStackScreenPage.syncError("n2").should("contain.text", "src/auth/mod.rs");
  prStackScreenPage.syncMergeBtn("n2").should("be.enabled");
});

it("clears a previous failure when a new pull is started", () => {
  // Given — a reason kept beside a pull that is now in flight reports a state that is no longer true
  const backend = aPrStackBackend({ resolutionByBranch: { [CHILD_BRANCH]: behindBy(3) } });
  let attempt = 0;
  mountAndOpen(
    backend.onUnary(ConnectionService.method.pullBaseIntoBranch, () => {
      attempt += 1;
      if (attempt === 1) throw new ConnectError("git fetch failed", Code.Internal);
      return aPullBaseIntoBranchResponse({ resolution: PULLED_IN });
    }),
  );
  prStackScreenPage.expandRow("n2");
  prStackScreenPage.clickSyncMerge("n2");
  prStackScreenPage.syncError("n2").should("exist");

  // When
  prStackScreenPage.clickSyncMerge("n2");

  // Then
  prStackScreenPage.syncError("n2").should("not.exist");
});

it("keeps a pull failure visible after the row is collapsed", () => {
  // Given
  openPrStackScreenWithRefusedPull("git fetch origin failed", {
    resolutionByBranch: { [CHILD_BRANCH]: behindBy(3) },
  });
  prStackScreenPage.expandRow("n2");
  prStackScreenPage.clickSyncMerge("n2");
  prStackScreenPage.syncError("n2").should("be.visible");

  // When
  prStackScreenPage.expandRow("n2");

  // Then — a reason the operator must expand a row to find is a fresh dead end
  prStackScreenPage.rowDetails("n2").should("not.be.visible");
  prStackScreenPage.syncError("n2").should("be.visible");
});

// ---------------------------------------------------------------------------
// A pull that landed locally but never reached the remote
// ---------------------------------------------------------------------------

it("reports a pull whose merge landed but whose push did not", () => {
  // Given — the daemon answers such a pull with success and a reason rather than rolling the merge
  // back (D32), so this is the one outcome that arrives looking exactly like a clean one
  openPrStackScreenWithUnpushedPull("remote rejected: protected branch hook declined", {
    resolutionByBranch: { [CHILD_BRANCH]: behindBy(3) },
  });
  prStackScreenPage.expandRow("n2");

  // When
  prStackScreenPage.clickSyncMerge("n2");

  // Then — stated as work that landed and has not been published, not as a pull that failed
  prStackScreenPage
    .syncError("n2")
    .should(
      "have.text",
      `Pulled ${ROOT_BRANCH} into ${CHILD_BRANCH} locally, but the push failed — ` +
        `${CHILD_BRANCH} on the remote (and its PR) does not have it yet: ` +
        "remote rejected: protected branch hook declined",
    );
});

it("keeps an unpushed pull reported after the row is collapsed", () => {
  // Given — the row reads "in sync" from here on, because locally it is
  openPrStackScreenWithUnpushedPull("remote rejected: protected branch hook declined", {
    resolutionByBranch: { [CHILD_BRANCH]: behindBy(3) },
  });
  prStackScreenPage.expandRow("n2");
  prStackScreenPage.clickSyncMerge("n2");
  prStackScreenPage.syncError("n2").should("be.visible");

  // When
  prStackScreenPage.expandRow("n2");

  // Then
  prStackScreenPage.rowDetails("n2").should("not.be.visible");
  prStackScreenPage.syncError("n2").should("be.visible");
});
