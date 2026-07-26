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
  worktree?: { exists: boolean; path?: string };
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
      },
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
    })),
  });
}
