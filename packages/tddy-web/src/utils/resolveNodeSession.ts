import type { SessionEntry } from "../gen/connection_pb";

/**
 * Resolves the child session that owns a planned node's branch — the PR-Stack view's
 * branch→session join. A node is "in progress" when a live session works its branch; the branch
 * is the durable link key (`node.branch` matched against each `SessionEntry.branch`).
 *
 * Returns `undefined` when the node has no branch yet or no session matches it. When more than one
 * session shares the branch (should not happen for a well-formed stack), an active session is
 * preferred over an inactive one.
 */
export function resolveNodeSession(
  node: { branch: string | null },
  sessions: SessionEntry[],
): SessionEntry | undefined {
  const branch = node.branch;
  if (!branch) return undefined;

  const matches = sessions.filter((s) => s.branch === branch);
  if (matches.length === 0) return undefined;

  return matches.find((s) => s.isActive) ?? matches[0];
}
