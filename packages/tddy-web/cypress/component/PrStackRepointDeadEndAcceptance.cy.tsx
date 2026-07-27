/**
 * Acceptance tests: recovering a planned PR that is stuck behind a base branch which no longer exists.
 *
 * The reported case: a predecessor's PR was merged on GitHub and its branch deleted, but the plan's own
 * `pr_status` — written by the orchestrator agent — still says `open`. The row therefore read "Missing
 * branch: <deleted branch>" with no Start-session button and no Repoint control (which was gated on the
 * recorded `merged` phase), and the node was unrecoverable.
 *
 * Two things change here. A blocked row keeps its **full information** and a **disabled** Start-session
 * button beside a warning naming each blocking issue — never a bare indicator in place of everything
 * (D16). And Repoint is offered for **any** unresolvable base, labelled with the branch the node will
 * land on, which is also the `target_base_branch` sent with the click (D17, D18, D20).
 *
 * PRD: docs/ft/coder/pr-stack-live-status.md § Repointing a dead-end planned PR (D16–D20).
 * Changeset: docs/dev/changesets.md (2026-07-26, pr-stack-repoint-dead-end).
 */

import React from "react";
import { Code, ConnectError } from "@connectrpc/connect";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import {
  ConnectionService,
  type ProjectEntry,
  type SessionEntry,
} from "../../src/gen/connection_pb";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";
import {
  aPlannedNode,
  aStackPlanJson,
  aBranchResolutionResponse,
  type BranchResolutionFixture,
  type StackNodeFixture,
} from "../support/rpc/prStackFixtures";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const ORCHESTRATOR_SESSION_ID = "pr-stack-session-8888-0000-0000-0000-000000000080";
const PROJECT_ID = "proj-pr-stack";
const DEFAULT_BRANCH = "origin/master";

/** The merged predecessor's branch — deleted from `origin` once its PR landed. */
const DELETED_BASE_BRANCH = "feature/attach-docs/attach-proto";
/** A second predecessor's branch, still open and still on `origin`. */
const LIVE_BASE_BRANCH = "feature/attach-docs/attach-store";

/**
 * The reported stack: `n1`'s PR was merged and its branch deleted, but the plan still records
 * `phase: "open"`, so nothing in the stored plan reveals that `n2` is stranded.
 */
const A_STRANDED_DEPENDENT: StackNodeFixture[] = [
  aPlannedNode({
    nodeId: "n1",
    title: "Start-session attachment proto",
    branch: DELETED_BASE_BRANCH,
    sessionId: "child-n1",
    prStatus: { phase: "open" },
  }),
  aPlannedNode({
    nodeId: "n2",
    title: "Copy attachments during StartSession",
    description: "Copies every attachment into the child session directory.",
    branchSuggestion: "feature/attach-docs/attach-start",
    parents: ["n1"],
  }),
];

/** The same plan after a repoint dropped `n1` from `n2`'s parents — `n2` is a root now. */
const A_REPOINTED_DEPENDENT: StackNodeFixture[] = [
  A_STRANDED_DEPENDENT[0],
  aPlannedNode({
    nodeId: "n2",
    title: "Copy attachments during StartSession",
    description: "Copies every attachment into the child session directory.",
    branchSuggestion: "feature/attach-docs/attach-start",
    parents: [],
  }),
];

function anOrchestratorSession(nodes: StackNodeFixture[]): Partial<SessionEntry> {
  return {
    sessionId: ORCHESTRATOR_SESSION_ID,
    createdAt: "2026-07-26T09:50:00Z",
    status: "idle",
    repoPath: "/home/dev/pr-stack-project",
    isActive: false,
    projectId: PROJECT_ID,
    recipe: "pr-stack",
    stackPlanJson: aStackPlanJson(1, nodes),
  };
}

function aProject(mainBranchRef: string): Partial<ProjectEntry> {
  return {
    projectId: PROJECT_ID,
    name: "pr-stack-project",
    gitUrl: "https://example.com/pr-stack.git",
    mainRepoPath: "/home/dev/pr-stack-project",
    mainBranchRef,
    daemonInstanceId: "local",
  };
}

/** The deleted base branch, plus a live PR on the stranded node's own predecessor. */
const A_DELETED_BASE: Record<string, BranchResolutionFixture> = {
  [DELETED_BASE_BRANCH]: {
    branch: DELETED_BASE_BRANCH,
    remote: { exists: false },
    pr: {
      exists: true,
      number: 351,
      url: "https://github.com/uppin/tddy-coder/pull/351",
      state: "merged",
    },
  },
};

interface PrStackScreenOptions {
  nodes: StackNodeFixture[];
  resolutionByBranch: Record<string, BranchResolutionFixture>;
  mainBranchRef?: string;
  /** The plan `RepointPlannedPr` returns; defaults to the plan the screen opened with. */
  repointedNodes?: StackNodeFixture[];
}

/** Everything but `RepointPlannedPr`, which is what the two openers differ on. */
function aPrStackBackend(options: PrStackScreenOptions) {
  return aSessionsDrawerBackend([anOrchestratorSession(options.nodes)])
    .onUnary(ConnectionService.method.listProjects, () => ({
      projects: [aProject(options.mainBranchRef ?? DEFAULT_BRANCH)],
    }))
    .onUnary(ConnectionService.method.queryBranch, (req: { branch: string }) =>
      aBranchResolutionResponse(options.resolutionByBranch[req.branch] ?? { branch: req.branch }),
    );
}

function mountAndOpenPrStackSession(backend: ReturnType<typeof aPrStackBackend>) {
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
  return backend;
}

/** Open the screen with a `RepointPlannedPr` that succeeds and returns the repointed plan. */
function openPrStackScreen(options: PrStackScreenOptions) {
  return mountAndOpenPrStackSession(
    aPrStackBackend(options).onUnary(ConnectionService.method.repointPlannedPr, () => ({
      stackPlanJson: aStackPlanJson(1, options.repointedNodes ?? options.nodes),
    })),
  );
}

/** Open the screen with a `RepointPlannedPr` the daemon refuses, carrying `message` as its reason. */
function openPrStackScreenWithRefusedRepoint(message: string, options: PrStackScreenOptions) {
  return mountAndOpenPrStackSession(
    aPrStackBackend(options).onUnary(ConnectionService.method.repointPlannedPr, () => {
      throw new ConnectError(message, Code.InvalidArgument);
    }),
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
// The recovery action
// ---------------------------------------------------------------------------

it("offers a Repoint control naming the default branch when the base branch was deleted from origin", () => {
  // Given / When — n1's PR merged and its branch was deleted; the plan still records "open"
  openPrStackScreen({ nodes: A_STRANDED_DEPENDENT, resolutionByBranch: A_DELETED_BASE });

  // Then — the operator can see exactly where the node will land before clicking
  prStackScreenPage.repointBtn("n2").should("have.text", `Repoint to ${DEFAULT_BRANCH}`);
});

it("sends the named target branch when Repoint is clicked", () => {
  // Given
  const backend = openPrStackScreen({
    nodes: A_STRANDED_DEPENDENT,
    resolutionByBranch: A_DELETED_BASE,
    repointedNodes: A_REPOINTED_DEPENDENT,
  });

  // When
  prStackScreenPage.clickRepoint("n2");

  // Then — the daemon is asked for exactly what the label promised
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ConnectionService.method.repointPlannedPr);
    expect(calls).to.have.length(1);
    expect(calls[0].sessionId).to.equal(ORCHESTRATOR_SESSION_ID);
    expect(calls[0].nodeId).to.equal("n2");
    expect(calls[0].targetBaseBranch).to.equal(DEFAULT_BRANCH);
  });
});

it("makes the node startable once the repoint has dropped its dead parent", () => {
  // Given
  openPrStackScreen({
    nodes: A_STRANDED_DEPENDENT,
    resolutionByBranch: A_DELETED_BASE,
    repointedNodes: A_REPOINTED_DEPENDENT,
  });

  // When
  prStackScreenPage.clickRepoint("n2");

  // Then — the returned plan re-renders the row unblocked, with nothing left to warn about
  prStackScreenPage.startSessionBtn("n2").should("be.enabled");
  prStackScreenPage.startWarning("n2").should("not.exist");
});

it("names the surviving predecessor's branch as the Repoint target when only one of two parents is dead", () => {
  // Given — n3 depends on n1 (branch gone from origin) and n2 (open, still pushed)
  const nodes = [
    aPlannedNode({
      nodeId: "n1",
      title: "Start-session attachment proto",
      branch: DELETED_BASE_BRANCH,
      prStatus: { phase: "open" },
    }),
    aPlannedNode({
      nodeId: "n2",
      title: "Session attachment storage",
      branch: LIVE_BASE_BRANCH,
      prStatus: { phase: "open" },
    }),
    aPlannedNode({
      nodeId: "n3",
      title: "Copy attachments during StartSession",
      parents: ["n1", "n2"],
    }),
  ];

  // When
  openPrStackScreen({
    nodes,
    resolutionByBranch: {
      ...A_DELETED_BASE,
      [LIVE_BASE_BRANCH]: {
        branch: LIVE_BASE_BRANCH,
        remote: { exists: true, sha: "4e2e8e8cf5de99f8485e518e925d382ae9275c76" },
      },
    },
  });

  // Then — repointing must not detach a predecessor that is still a usable base
  prStackScreenPage.repointBtn("n3").should("have.text", `Repoint to ${LIVE_BASE_BRANCH}`);
});

it("reads 'Repoint to default branch' when the project records no default branch", () => {
  // Given / When — a legacy project with no stored main_branch_ref
  openPrStackScreen({
    nodes: A_STRANDED_DEPENDENT,
    resolutionByBranch: A_DELETED_BASE,
    mainBranchRef: "",
  });

  // Then — only the label degrades; the daemon resolves the real ref when clicked
  prStackScreenPage.repointBtn("n2").should("have.text", "Repoint to default branch");
});

it("offers no Repoint control on a node whose base branch is on origin", () => {
  // Given / When — n1's branch is pushed and open, so n2 has a usable base
  openPrStackScreen({
    nodes: A_STRANDED_DEPENDENT,
    resolutionByBranch: {
      [DELETED_BASE_BRANCH]: {
        branch: DELETED_BASE_BRANCH,
        remote: { exists: true, sha: "4e2e8e8cf5de99f8485e518e925d382ae9275c76" },
      },
    },
  });

  // Then — the control must not become ambient noise on a healthy stack
  prStackScreenPage.repointBtn("n2").should("not.exist");
  prStackScreenPage.startSessionBtn("n2").should("be.enabled");
});

// ---------------------------------------------------------------------------
// A blocked row is still a full row
// ---------------------------------------------------------------------------

it("keeps its title, description, planned branch and base branch on a row that cannot be started", () => {
  // Given / When
  openPrStackScreen({ nodes: A_STRANDED_DEPENDENT, resolutionByBranch: A_DELETED_BASE });

  // Then — being blocked must never cost the operator the information they need to act. The row is
  // the whole card, so `contain.text`: it also holds the base line, the CTA and the warning.
  prStackScreenPage
    .plannedPrRow("n2")
    .should("contain.text", "Copy attachments during StartSession")
    .and("contain.text", "Copies every attachment into the child session directory.");
  prStackScreenPage
    .plannedBranchName("n2")
    .should("have.text", "planned: feature/attach-docs/attach-start");
  prStackScreenPage.baseBranch("n2").should("have.text", `base: ${DELETED_BASE_BRANCH}`);
});

it("disables Start session and warns that the base branch is not on origin", () => {
  // Given / When
  openPrStackScreen({ nodes: A_STRANDED_DEPENDENT, resolutionByBranch: A_DELETED_BASE });

  // Then — the CTA is disabled with the reason, not replaced by it
  prStackScreenPage.startSessionBtn("n2").should("be.disabled");
  prStackScreenPage
    .startWarning("n2")
    .should("have.text", `Base branch ${DELETED_BASE_BRANCH} is not on origin`);
});

it("carries every blocker message as the disabled Start-session button's tooltip", () => {
  // Given / When
  openPrStackScreen({ nodes: A_STRANDED_DEPENDENT, resolutionByBranch: A_DELETED_BASE });

  // Then — hovering the control that cannot be pressed answers why, without hunting for the warning
  prStackScreenPage
    .startSessionBtn("n2")
    .should("have.attr", "title", `Base branch ${DELETED_BASE_BRANCH} is not on origin`);
});

it("warns with every reason at once when a row is blocked for two of them", () => {
  // Given — n3 depends on n1 (branch gone from origin) and n2 (planned only, so it owns no ref)
  const nodes = [
    aPlannedNode({
      nodeId: "n1",
      title: "Start-session attachment proto",
      branch: DELETED_BASE_BRANCH,
      prStatus: { phase: "open" },
    }),
    aPlannedNode({
      nodeId: "n2",
      title: "Session attachment storage",
      branchSuggestion: LIVE_BASE_BRANCH,
    }),
    aPlannedNode({
      nodeId: "n3",
      title: "Copy attachments during StartSession",
      parents: ["n1", "n2"],
    }),
  ];

  // When
  openPrStackScreen({ nodes, resolutionByBranch: A_DELETED_BASE });

  // Then — naming one reason and hiding the other is what left the operator guessing (D16). The two
  // messages render as adjacent block spans, so the element's text is their concatenation.
  prStackScreenPage
    .startWarning("n3")
    .should(
      "have.text",
      `Session attachment storage has not created its branch yetBase branch ${DELETED_BASE_BRANCH} is not on origin`,
    );
});

it("offers a Repoint control on a node blocked by a predecessor that owns no branch yet", () => {
  // Given — nothing has merged and no branch was deleted; n1 was simply never started
  const nodes = [
    aPlannedNode({
      nodeId: "n1",
      title: "Start-session attachment proto",
      branchSuggestion: DELETED_BASE_BRANCH,
    }),
    aPlannedNode({
      nodeId: "n2",
      title: "Copy attachments during StartSession",
      parents: ["n1"],
    }),
  ];

  // When
  openPrStackScreen({ nodes, resolutionByBranch: {} });

  // Then — Repoint is offered for *any* unresolvable base (D17), and no parent can serve as one here,
  // so the node would land on the default branch
  prStackScreenPage.repointBtn("n2").should("have.text", `Repoint to ${DEFAULT_BRANCH}`);
});

it("shows no start warning on a node whose child session already exists", () => {
  // Given — n2 was already spawned, and its base branch is gone from origin
  const nodes = [
    A_STRANDED_DEPENDENT[0],
    aPlannedNode({
      nodeId: "n2",
      title: "Copy attachments during StartSession",
      sessionId: "child-n2",
      parents: ["n1"],
    }),
  ];

  // When
  openPrStackScreen({ nodes, resolutionByBranch: A_DELETED_BASE });

  // Then — the blockers exist to explain a Start-session button that cannot be pressed; this row shows
  // a status chip instead, and a base its child will never be created from is not news
  prStackScreenPage.statusChip("n2").should("exist");
  prStackScreenPage.startWarning("n2").should("not.exist");
});

// ---------------------------------------------------------------------------
// A refused repoint must say so
// ---------------------------------------------------------------------------

/** A daemon refusal of a stale label that names no acceptable base. */
const A_REJECTED_TARGET_REASON =
  "target_base_branch 'origin/master' names neither the default branch 'origin/main' nor any parent's branch";

it("surfaces the daemon's reason when a repoint is refused", () => {
  // Given
  openPrStackScreenWithRefusedRepoint(A_REJECTED_TARGET_REASON, {
    nodes: A_STRANDED_DEPENDENT,
    resolutionByBranch: A_DELETED_BASE,
  });

  // When
  prStackScreenPage.clickRepoint("n2");

  // Then — a refusal the operator cannot see is the dead end this whole feature removes. `have.text`
  // is impossible here: `ConnectError.message` prepends "[invalid_argument] " to the daemon's reason.
  prStackScreenPage.repointError("n2").should("contain.text", A_REJECTED_TARGET_REASON);
});

it("leaves the row blocked when the repoint was refused", () => {
  // Given
  openPrStackScreenWithRefusedRepoint(
    "could not resolve default branch: no origin/master, origin/main, or origin/HEAD",
    { nodes: A_STRANDED_DEPENDENT, resolutionByBranch: A_DELETED_BASE },
  );

  // When
  prStackScreenPage.clickRepoint("n2");

  // Then — nothing was persisted, so the row must not read as recovered
  prStackScreenPage.startSessionBtn("n2").should("be.disabled");
  prStackScreenPage
    .startWarning("n2")
    .should("have.text", `Base branch ${DELETED_BASE_BRANCH} is not on origin`);
});
