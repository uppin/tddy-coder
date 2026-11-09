/**
 * Fixture builder for `SessionEntry.stackPlanJson` — the JSON-serialized `Stack`
 * (`tddy_core::changeset::Stack`) a "pr-stack" orchestrator session carries once it has a plan.
 *
 * Field names are snake_case to match the Rust struct's default serde encoding
 * (no `rename_all` on `Stack`/`StackNode` — see `packages/tddy-core/src/changeset.rs`).
 */

export interface StackNodeFixture {
  nodeId: string;
  title: string;
  description?: string;
  branchSuggestion?: string | null;
  branch?: string | null;
  sessionId?: string | null;
  parents?: string[];
  prStatus?: { phase: string; url?: string | null; error?: string | null } | null;
  childState?: string | null;
  internalStatus?: { kind: string; note?: string | null; source: string } | null;
  /** Recipe the child session is started with. Absent means the plan left it unset ("tdd"). */
  childRecipe?: string | null;
  /**
   * The node's persisted row position. Left unset by `aPlannedNode` on purpose: a plan authored
   * before display order existed carries none, so every scenario that does not opt in keeps
   * exercising the topological fallback.
   */
  displayOrder?: number | null;
}

/** A planned PR node with sensible defaults — override only what a scenario cares about. */
export function aPlannedNode(overrides: Partial<StackNodeFixture> & { nodeId: string; title: string }): StackNodeFixture {
  return {
    description: "",
    branchSuggestion: null,
    branch: null,
    sessionId: null,
    parents: [],
    prStatus: null,
    childState: null,
    ...overrides,
  };
}

/** A `QueryBranch` resolution fixture — the session/worktree/remote/PR resolved for one branch. */
export interface BranchResolutionFixture {
  branch: string;
  session?: { exists: boolean; sessionId?: string; isActive?: boolean; status?: string };
  worktree?: {
    exists: boolean;
    path?: string;
    /** Uncommitted changes to tracked files. A pull must prompt before touching such a worktree. */
    dirty?: boolean;
    dirtyPaths?: string[];
  };
  /**
   * How the branch stands against the base it is stacked on. Left **unset** by default: a zeroed
   * message is byte-identical to "in sync with nothing behind", so defaulting it would make every
   * scenario's rows claim a clean base comparison the fixture never described.
   */
  baseSync?: {
    baseBranch?: string;
    behindCount?: number;
    aheadCount?: number;
    hasConflicts?: boolean;
    conflictedPaths?: string[];
    /** True when the comparison could not be made — never the same as "clean". */
    unavailable?: boolean;
    unavailableReason?: string;
    baseRef?: string;
    headRef?: string;
  };
  pr?: {
    exists: boolean;
    number?: number;
    url?: string;
    state?: string;
    /** True when the lookup could not be performed — distinct from "no PR exists". */
    unavailable?: boolean;
    unavailableReason?: string;
  };
  /** The branch on `origin` — whether a descendant's worktree can be based onto it. */
  remote?: { exists: boolean; sha?: string };
}

/** Build a `QueryBranchResponse`-shaped object for an in-memory `queryBranch` stub. */
export function aBranchResolutionResponse(fx: BranchResolutionFixture) {
  return {
    resolution: {
      branch: fx.branch,
      session: {
        exists: fx.session?.exists ?? false,
        sessionId: fx.session?.sessionId ?? "",
        isActive: fx.session?.isActive ?? false,
        status: fx.session?.status ?? "",
      },
      worktree: {
        exists: fx.worktree?.exists ?? false,
        path: fx.worktree?.path ?? "",
        dirty: fx.worktree?.dirty ?? false,
        dirtyPaths: fx.worktree?.dirtyPaths ?? [],
      },
      // Emitted only when the scenario describes one — an absent leg is "unknown", which is what a
      // daemon that predates base sync sends and what an unanswered probe means.
      ...(fx.baseSync
        ? {
            baseSync: {
              baseBranch: fx.baseSync.baseBranch ?? "",
              behindCount: fx.baseSync.behindCount ?? 0,
              aheadCount: fx.baseSync.aheadCount ?? 0,
              hasConflicts: fx.baseSync.hasConflicts ?? false,
              conflictedPaths: fx.baseSync.conflictedPaths ?? [],
              unavailable: fx.baseSync.unavailable ?? false,
              unavailableReason: fx.baseSync.unavailableReason ?? "",
              baseRef: fx.baseSync.baseRef ?? "",
              headRef: fx.baseSync.headRef ?? "",
            },
          }
        : {}),
      pr: {
        exists: fx.pr?.exists ?? false,
        number: BigInt(fx.pr?.number ?? 0),
        url: fx.pr?.url ?? "",
        state: fx.pr?.state ?? "",
        unavailable: fx.pr?.unavailable ?? false,
        unavailableReason: fx.pr?.unavailableReason ?? "",
      },
      remote: {
        exists: fx.remote?.exists ?? false,
        sha: fx.remote?.sha ?? "",
      },
    },
  };
}

/**
 * Build a `PullBaseIntoBranchResponse`-shaped object for an in-memory `pullBaseIntoBranch` stub —
 * the re-resolved branch plus what actually happened to it.
 *
 * `pushed` defaults to `true` because the daemon answers a pull that fully landed that way. A pull
 * whose local merge or rebase succeeded but whose push did not is still a *successful* call carrying
 * `pushed = false` and a reason (D32) — a state a scenario opts into deliberately, never one a
 * fixture default should describe by accident.
 */
export function aPullBaseIntoBranchResponse(fx: {
  resolution: BranchResolutionFixture;
  strategy?: "merge" | "rebase";
  changed?: boolean;
  pushed?: boolean;
  pushError?: string;
}) {
  return {
    ...aBranchResolutionResponse(fx.resolution),
    strategy: fx.strategy ?? "merge",
    changed: fx.changed ?? true,
    pushed: fx.pushed ?? true,
    pushError: fx.pushError ?? "",
  };
}

/** Serialize a `Stack` fixture to the `stack_plan_json` wire format. */
export function aStackPlanJson(version: number, nodes: StackNodeFixture[]): string {
  return JSON.stringify({
    version,
    nodes: nodes.map((n) => ({
      node_id: n.nodeId,
      title: n.title,
      description: n.description ?? "",
      branch_suggestion: n.branchSuggestion ?? null,
      branch: n.branch ?? null,
      session_id: n.sessionId ?? null,
      parents: n.parents ?? [],
      pr_status: n.prStatus ?? null,
      child_state: n.childState ?? null,
      internal_status: n.internalStatus ?? null,
      child_recipe: n.childRecipe ?? null,
      display_order: n.displayOrder ?? null,
    })),
  });
}
