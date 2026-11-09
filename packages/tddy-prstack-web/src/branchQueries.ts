import { resolveStackBase } from "./deriveStackBaseBranch";
import type { StackNode } from "./stackPlan";

/**
 * What a query is asking about — an existing ref, or a name nothing has created yet.
 *
 * The discriminator is what lets a reader take only the answers a planned name can honestly give.
 * The `pr` leg asks the GitHub API by head branch and is host-independent; every other leg is read
 * off the queried daemon's own disk and describes a ref that, for a `planned-name`, does not exist
 * anywhere (D1, D41).
 */
export type BranchQueryKind = "owned-branch" | "planned-name";

/** One `QueryBranch` call: a branch, and the base its standing is measured against. */
export interface BranchQuery {
  branch: string;
  /**
   * The base the daemon compares `branch` against. Empty when no ancestor owns a ref — an unnamed
   * base is reported back as unavailable rather than substituted with a guess (D29), since the
   * number beside a row must describe the same base the row's own base line shows. Always empty for
   * a `planned-name` query: a suggestion is not a ref, so there is no comparison to make.
   */
  baseBranch: string;
  /** Whether `branch` is a ref a node owns, or a node's planned name for one. */
  kind: BranchQueryKind;
}

/**
 * What the PR-Stack screen asks `QueryBranch` to resolve: one call per branch a node owns, each
 * paired with that node's base, plus one per planned name a branchless node still carries.
 *
 * The set used to be branch names alone — the branches nodes own plus each node's *ancestor* base —
 * which left a **root** node, whose base is the project's default branch, with no comparison at all.
 * That is the one comparison every stack has.
 *
 * Pairing adds no calls of its own: every ancestor base is by definition some node's own branch, so
 * it is already in the set. That matters because each call reaches GitHub for that branch's PR
 * status, and polling more branches than the stack owns would spend the hourly rate limit on
 * comparisons nobody reads.
 *
 * A node that owns **no** branch was previously skipped entirely, so a PR opened for its planned name
 * — by a session on another host, or by a session that has since ended — was invisible in the row
 * that exists to track it. Such a node is queried on its `branch_suggestion` and tagged
 * `planned-name`, whose answer the view reads for its `pr` leg alone (D41). The query names no base
 * for the same reason: there is no ref to compare. An owned query always wins over a planned one for
 * the same name — a ref that exists answers more than a name — and one name shared by two branchless
 * nodes is a malformed plan, not a reason to double the GitHub request rate.
 *
 * Sorted by branch so the result is a stable poll key: an unrelated re-render that hands over an
 * equivalent set must not re-subscribe the poll.
 */
export function buildBranchQueries(nodes: StackNode[], defaultBranch: string): BranchQuery[] {
  const byBranch = new Map<string, BranchQuery>();

  for (const node of nodes) {
    if (!node.branch) continue;
    const base = resolveStackBase(node, nodes);
    const baseBranch =
      base.kind === "ancestor-branch"
        ? base.branch
        : base.kind === "default-branch"
          ? defaultBranch
          : "";
    const existing = byBranch.get(node.branch);
    // Two nodes claiming one branch is a malformed plan, not a state to resolve twice. Prefer the
    // entry that names a base: a comparison against a base beats no comparison at all.
    if (existing && existing.baseBranch) continue;
    byBranch.set(node.branch, { branch: node.branch, baseBranch, kind: "owned-branch" });
  }

  // Second pass, so an owned branch is already in the map by the time a node that merely plans the
  // same name is considered — order in the plan must not decide which kind the poll set carries.
  for (const node of nodes) {
    if (node.branch) continue;
    const planned = node.branchSuggestion;
    if (!planned || byBranch.has(planned)) continue;
    byBranch.set(planned, { branch: planned, baseBranch: "", kind: "planned-name" });
  }

  return [...byBranch.values()].sort((a, b) => a.branch.localeCompare(b.branch));
}

/**
 * The branch names among `queries` that were asked as **planned names** — the only resolutions a row
 * may read a `plannedPr` from.
 *
 * The discriminator has to be read here rather than re-derived from the node, because the two
 * disagree in exactly the case that matters. `buildBranchQueries` emits one query per name and lets
 * an owned branch win it, so a branchless node whose `branch_suggestion` collides with a branch some
 * *other* node created still satisfies "no branch, has a suggestion" while the answer under that name
 * describes the other node's ref. Reading it would render that node's live PR number and state in a
 * row that owns no branch and has no session — a link to somebody else's PR, presented as its own.
 *
 * PRD: docs/ft/coder/pr-stack-live-status.md § Cross-host planned PRs (D41).
 */
export function plannedNameBranches(queries: BranchQuery[]): ReadonlySet<string> {
  return new Set(queries.filter((q) => q.kind === "planned-name").map((q) => q.branch));
}
