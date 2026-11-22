/**
 * Parsing + ordering helpers for `SessionEntry.workQueueJson` — the JSON-serialized `Stack`
 * (`tddy_core::changeset::Stack`) an orchestrator session carries once it has a plan.
 *
 * Field names are snake_case to match the Rust struct's default serde encoding (no
 * `rename_all` on `Stack` / `WorkItem` — see `packages/tddy-core/src/changeset.rs`).
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

export interface WorkItem {
  itemId: string;
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
  /**
   * The node's persisted row position — the operator's reading order, which is a different fact from
   * the dependency graph and does not change when the graph does. `null` for a node the plan does not
   * order (a stack authored before display order existed), which falls back to topological order —
   * see `orderWorkItems`.
   */
  displayOrder: number | null;
  /** The existing PR assigned to this item (`#workqueue` 6/14); its head branch is the item's branch. */
  pr: AssignedPr | null;
  /** The tddy sessions linked to this item, in link order (`#workqueue` 6/14). */
  linkedSessions: LinkedSession[];
  /** Free-form tags on this item (`#workqueue` 7/14); the queue view filters by them. */
  labels: string[];
  /** Keyed markers with a short text and a tone (`#workqueue` 7/14). */
  badges: WorkItemBadge[];
}

export type BadgeTone = "info" | "success" | "warning" | "danger";

export interface WorkItemBadge {
  key: string;
  text: string;
  tone: BadgeTone;
}

export interface AssignedPr {
  number: number;
  url: string;
  headBranch: string;
}

export interface LinkedSession {
  sessionId: string;
  /** The daemon holding the session; empty for the queue's own host. */
  daemonInstanceId: string;
}

export interface Stack {
  version: number;
  nodes: WorkItem[];
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
  display_order?: number | null;
  pr?: { number: number; url: string; head_branch: string } | null;
  linked_sessions?: { session_id: string; daemon_instance_id?: string }[];
  labels?: string[];
  badges?: WorkItemBadge[];
}

interface WireStack {
  version?: number;
  nodes?: WireStackNode[];
}

/** Parse `SessionEntry.workQueueJson` into a `Stack`. Empty/invalid input yields an empty stack. */
export function parseWorkQueue(workQueueJson: string | undefined | null): Stack {
  if (!workQueueJson) return { version: 0, nodes: [] };
  let parsed: WireStack;
  try {
    parsed = JSON.parse(workQueueJson) as WireStack;
  } catch {
    return { version: 0, nodes: [] };
  }
  const nodes = (parsed.nodes ?? []).map((n): WorkItem => ({
    itemId: n.node_id,
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
    // Position 0 is a real position, so an absent field is distinguished by type rather than by
    // falsiness: `?? null` would read the first row's own position as "this node has no position".
    displayOrder: typeof n.display_order === "number" ? n.display_order : null,
    pr: n.pr ? { number: n.pr.number, url: n.pr.url, headBranch: n.pr.head_branch } : null,
    linkedSessions: (n.linked_sessions ?? []).map((linked) => ({
      sessionId: linked.session_id,
      daemonInstanceId: linked.daemon_instance_id ?? "",
    })),
    labels: n.labels ?? [],
    badges: n.badges ?? [],
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
export function topoSortWorkItems(nodes: WorkItem[]): WorkItem[] {
  const remaining = [...nodes];
  const placedIds = new Set<string>();
  const ordered: WorkItem[] = [];

  while (remaining.length > 0) {
    const ready = remaining.filter((n) => n.parents.every((p) => placedIds.has(p)));
    if (ready.length === 0) {
      // Cycle or dangling parent reference — append the rest as-is rather than looping forever.
      ordered.push(...remaining);
      break;
    }
    for (const node of ready) {
      ordered.push(node);
      placedIds.add(node.itemId);
      const idx = remaining.indexOf(node);
      remaining.splice(idx, 1);
    }
  }

  return ordered;
}
