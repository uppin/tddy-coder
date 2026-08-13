import type { SessionEntry } from "../gen/connection_pb";

/**
 * Returns sessions that are referenced as `orchestratorSessionId` by at least one other session
 * in the list — i.e. sessions currently acting as PR-stack orchestrators.
 *
 * Useful for populating the parent-picker `<select>` in the new-session screen.
 */
export function stackParentCandidates(sessions: SessionEntry[]): SessionEntry[] {
  // Collect all orchestratorSessionIds referenced by child sessions.
  const referencedIds = new Set<string>();
  for (const s of sessions) {
    if (s.orchestratorSessionId.length > 0) {
      referencedIds.add(s.orchestratorSessionId);
    }
  }
  if (referencedIds.size === 0) return [];

  // Return sessions whose sessionId is in the referenced set (deduped by Set lookup).
  const seen = new Set<string>();
  const result: SessionEntry[] = [];
  for (const s of sessions) {
    if (referencedIds.has(s.sessionId) && !seen.has(s.sessionId)) {
      seen.add(s.sessionId);
      result.push(s);
    }
  }
  return result;
}

export const PR_STACK_RECIPES = ["pr-stack", "orchestrate-pr-stack", "plan-pr-stack"] as const;

/**
 * Returns sessions eligible to be selected as a PR-stack parent: those with a PR-stack
 * recipe that are not themselves children of another orchestrator.
 *
 * Useful for populating the parent-picker <select> in the new-session screen.
 */
export function prStackOrchestrators(sessions: SessionEntry[]): SessionEntry[] {
  return sessions.filter(
    (s) =>
      (PR_STACK_RECIPES as readonly string[]).includes(s.recipe) &&
      s.orchestratorSessionId.length === 0,
  );
}

/**
 * Returns the sessions that own a branch, in list order.
 *
 * Useful for populating the "Base the stack on" <select> in the new-session screen: a new PR-stack
 * orchestrator's stack is seeded with one existing session's branch, and a session that has not
 * created its branch yet owns no ref for the seeded node — or any descendant — to be based on.
 *
 * Owning a branch is necessary but not sufficient — see {@link stackBaseSessionCandidates}, which is
 * what the picker actually offers.
 */
export function sessionsWithBranch(sessions: SessionEntry[]): SessionEntry[] {
  return sessions.filter((s) => s.branch.length > 0);
}

/** Where the orchestrator under creation will live: the project and host its form resolves to. */
export interface StackBaseScope {
  /** The form's effective `projectId`. */
  readonly projectId: string;
  /** The form's effective `daemonInstanceId` (the host the session is created on). */
  readonly daemonInstanceId: string;
}

/**
 * Returns the sessions whose branch may seed a new PR-stack orchestrator's stack, in list order.
 *
 * Narrower than {@link sessionsWithBranch} on three counts, each of which is a branch the stack could
 * otherwise not act on:
 *
 * - **Same project.** Every descendant node's worktree is created off `origin/<base branch>` inside
 *   the orchestrator's project, so a branch from a different repository cannot be stacked on — the
 *   failure would land much later, as a git error, on an orchestrator that already looks seeded.
 * - **Same host.** A stack's branches are operated on the host the orchestrator runs on; a session on
 *   another daemon's checkout is not a ref this one can fetch or repoint.
 * - **Not already stacked.** A session that is already a node of another orchestrator's stack
 *   (`orchestratorSessionId` set) would end up with two orchestrators holding repoint and pull
 *   authority over one branch.
 *
 * The daemon refuses all three again before it spawns (`validate_stack_seed_base_session`) — this is
 * what keeps the operator from choosing a refusal in the first place, not what enforces it.
 *
 * An empty `projectId` in the scope offers **nothing**: with no project resolved there is no
 * repository for a base session to share, and matching on the empty string would offer every session
 * whose project is also unknown.
 */
export function stackBaseSessionCandidates(
  sessions: SessionEntry[],
  scope: StackBaseScope,
): SessionEntry[] {
  if (scope.projectId.length === 0) return [];
  return sessionsWithBranch(sessions).filter(
    (s) =>
      s.projectId === scope.projectId &&
      s.daemonInstanceId === scope.daemonInstanceId &&
      s.orchestratorSessionId.length === 0,
  );
}
