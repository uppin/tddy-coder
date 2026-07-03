/**
 * Parsing + ordering helpers for `SessionEntry.stackPlanJson` — the JSON-serialized `Stack`
 * (`tddy_core::changeset::Stack`) an orchestrator session carries once it has a plan.
 *
 * Field names are snake_case to match the Rust struct's default serde encoding (no
 * `rename_all` on `Stack` / `StackNode` — see `packages/tddy-core/src/changeset.rs`).
 */

export interface PrStatus {
  phase: string;
  url?: string | null;
  error?: string | null;
}

/** Action-needed signal for a node (needs-repoint, has-conflicts, ready-to-merge, …). */
export interface PrInternalStatus {
  kind: string;
  note: string | null;
  source: string;
}

export interface StackNode {
  nodeId: string;
  title: string;
  description: string;
  branchSuggestion: string | null;
  branch: string | null;
  sessionId: string | null;
  parents: string[];
  prStatus: PrStatus | null;
  childState: string | null;
  /** Recipe the child session should be started with. Defaults to "tdd" when unset by the plan. */
  childRecipe: string;
  internalStatus: PrInternalStatus | null;
}

export interface Stack {
  version: number;
  nodes: StackNode[];
}

interface WireStackNode {
  node_id: string;
  title: string;
  description?: string;
  branch_suggestion?: string | null;
  branch?: string | null;
  session_id?: string | null;
  parents?: string[];
  pr_status?: PrStatus | null;
  child_state?: string | null;
  child_recipe?: string | null;
  internal_status?: PrInternalStatus | null;
}

interface WireStack {
  version?: number;
  nodes?: WireStackNode[];
}

/** Parse `SessionEntry.stackPlanJson` into a `Stack`. Empty/invalid input yields an empty stack. */
export function parseStackPlan(stackPlanJson: string | undefined | null): Stack {
  if (!stackPlanJson) return { version: 0, nodes: [] };
  let parsed: WireStack;
  try {
    parsed = JSON.parse(stackPlanJson) as WireStack;
  } catch {
    return { version: 0, nodes: [] };
  }
  const nodes = (parsed.nodes ?? []).map((n): StackNode => ({
    nodeId: n.node_id,
    title: n.title,
    description: n.description ?? "",
    branchSuggestion: n.branch_suggestion ?? null,
    branch: n.branch ?? null,
    sessionId: n.session_id ?? null,
    parents: n.parents ?? [],
    prStatus: n.pr_status ?? null,
    childState: n.child_state ?? null,
    childRecipe: n.child_recipe ?? "tdd",
    internalStatus: n.internal_status ?? null,
  }));
  return { version: parsed.version ?? 0, nodes };
}

/**
 * Sort stack nodes into topological order — parents (roots) before their dependents.
 *
 * Simple repeated-pass algorithm: on each pass, collect every not-yet-placed node whose
 * `parents` are all already placed, appending them (preserving relative input order among
 * ties). Any node whose dependencies never resolve (cycle / missing parent) is appended at
 * the end in its original relative order, rather than dropped, so no planned PR silently
 * disappears from the list.
 */
export function topoSortStackNodes(nodes: StackNode[]): StackNode[] {
  const remaining = [...nodes];
  const placedIds = new Set<string>();
  const ordered: StackNode[] = [];

  while (remaining.length > 0) {
    const ready = remaining.filter((n) => n.parents.every((p) => placedIds.has(p)));
    if (ready.length === 0) {
      // Cycle or dangling parent reference — append the rest as-is rather than looping forever.
      ordered.push(...remaining);
      break;
    }
    for (const node of ready) {
      ordered.push(node);
      placedIds.add(node.nodeId);
      const idx = remaining.indexOf(node);
      remaining.splice(idx, 1);
    }
  }

  return ordered;
}
