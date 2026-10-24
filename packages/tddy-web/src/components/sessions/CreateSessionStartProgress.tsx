import type { StartProgressState } from "./startProgress";

export interface CreateSessionStartProgressProps {
  /** The host doing the work: the codebase host on a split start, the session's host otherwise. */
  hostLabel: string;
  progress: StartProgressState;
}

/**
 * The create pane's view of a streamed start: "Creating session on <host>", then each step the host
 * reported — Cloning repository, Worktree, Semantic index, Agent — marked in progress, done or failed,
 * and under the clone step git's stage, a percent bar, bytes received and throughput.
 *
 * Feature docs: docs/ft/web/1-WIP/PRD-2026-10-24-start-progress-progress-ui.md
 */
export function CreateSessionStartProgress({ hostLabel, progress }: CreateSessionStartProgressProps) {
  // TODO(progress-ui): implement
  void hostLabel;
  void progress;
  return null;
}
