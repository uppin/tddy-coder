/**
 * Parse the `session` metadata block published by a tddy-coder LiveKit participant.
 *
 * The coder publishes `{ "session": { workflow_goal, workflow_state, elapsed_display, agent,
 * model, activity_status, recipe, repo_path, pending_elicitation } }` on its participant
 * metadata (shallow-merged with `owned_project_count` / `codex_oauth`). This parser tolerates
 * missing keys and older participants that carry no `session` block at all.
 *
 * Changeset: `2026-07-12-fast-session-change`
 */

export interface SessionMetadata {
  workflowGoal: string;
  workflowState: string;
  agent: string;
  model: string;
  activityStatus: string;
  recipe: string;
  repoPath: string;
  elapsedDisplay: string;
  pendingElicitation: boolean;
}

function str(value: unknown): string {
  return typeof value === "string" ? value : "";
}

function bool(value: unknown): boolean {
  return typeof value === "boolean" ? value : false;
}

/**
 * Parse the `session` block from a participant's metadata JSON.
 *
 * Returns `null` when the metadata is empty/whitespace, not valid JSON, or has no `session`
 * object (e.g. an older participant advertising only `owned_project_count`). Present keys are
 * parsed; absent keys default to empty string / `false`.
 */
export function parseSessionParticipantMetadata(metadataJson: string): SessionMetadata | null {
  const trimmed = metadataJson?.trim();
  if (!trimmed) return null;
  let root: unknown;
  try {
    root = JSON.parse(trimmed);
  } catch {
    return null;
  }
  const session = (root as { session?: unknown } | null)?.session;
  if (!session || typeof session !== "object") return null;
  const s = session as Record<string, unknown>;
  return {
    workflowGoal: str(s.workflow_goal),
    workflowState: str(s.workflow_state),
    agent: str(s.agent),
    model: str(s.model),
    activityStatus: str(s.activity_status),
    recipe: str(s.recipe),
    repoPath: str(s.repo_path),
    elapsedDisplay: str(s.elapsed_display),
    pendingElicitation: bool(s.pending_elicitation),
  };
}
