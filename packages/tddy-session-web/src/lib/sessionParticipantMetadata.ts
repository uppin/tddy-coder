/**
 * Parse the `session` metadata block published by a session's LiveKit participant.
 *
 * The publisher (`tddy-coder`, and the daemon for a claude-cli session) publishes
 * `{ "session": { workflow_goal, workflow_state, elapsed_display, agent, model, activity_status,
 * recipe, repo_path, pending_elicitation, session_id, orchestrator_session_id, stack_node_id,
 * branch } }` on its participant metadata (shallow-merged with `owned_project_count` /
 * `codex_oauth`). This parser tolerates missing keys and older participants that carry no `session`
 * block at all.
 *
 * The last four keys are the session's **stack association**. Presence is the only signal that
 * crosses a host boundary — `ListSessions` answers for one daemon's own sessions tree — so this
 * block is the only place the PR-Stack view can learn which planned PR a session on another host is
 * working (D37).
 *
 * Changeset: `2026-07-12-fast-session-change`, `2026-08-30-cross-host-planned-pr-visibility`
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
  /** The session this participant is. Empty for a publisher that names none. */
  sessionId: string;
  /** The pr-stack orchestrator that spawned it, or empty when the session is nobody's stack child. */
  orchestratorSessionId: string;
  /** The planned node it materializes, unique within its orchestrator's plan and nowhere else. */
  stackNodeId: string;
  /** The branch it created — the join key every same-host PR-stack lookup already uses. */
  branch: string;
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
 *
 * An absent association key therefore reads as empty, never as a wildcard: a partial match on a node
 * id would let one stack's row claim another stack's session, since node ids are unique within one
 * plan only.
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
    sessionId: str(s.session_id),
    orchestratorSessionId: str(s.orchestrator_session_id),
    stackNodeId: str(s.stack_node_id),
    branch: str(s.branch),
  };
}
