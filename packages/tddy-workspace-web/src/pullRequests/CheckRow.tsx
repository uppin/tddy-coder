/**
 * One check run (`#workqueue` 4/14): queued, running with its elapsed time, or completed with its
 * conclusion and duration, linking to its details. Reused by work-item rows (n6).
 *
 * Test id: `session-pr-check-<name>`.
 */
import type { PullRequestCheck } from "tddy-client-web/gen/worktree_pb";

export interface CheckRowProps {
  check: PullRequestCheck;
  /** Milliseconds since the epoch, for a running check's elapsed time. */
  nowMs: number;
}

export function CheckRow(_props: CheckRowProps) {
  // TODO(n4 session-pr-tabs): implement
  return null;
}
