/** `StartSessionRequest.session_role` the **Session orchestrator** switch asks for. */
export const ORCHESTRATOR_SESSION_ROLE = "orchestrator";

export interface CreateSessionOrchestratorFieldProps {
  /** The switch is on: the start asks for {@link ORCHESTRATOR_SESSION_ROLE}. */
  orchestrator: boolean;
  setOrchestrator: (orchestrator: boolean) => void;
}

/**
 * The New-session form's **Session orchestrator** switch (`#orchestrators` 1/11), test id
 * `create-session-orchestrator-toggle`. Offered only for an agent whose spec declares
 * `project_home_main`; on, the form asks for `session_role: "orchestrator"` and hides the placement,
 * branch and stack fields — the daemon places an orchestrator on the project's home worktree.
 *
 * Its own component so `CreateSessionPane.tsx` (over budget, see
 * `docs/code-issues/oversized-file-create-session-pane.md`) does not grow.
 *
 * PRD: docs/ft/daemon/1-WIP/PRD-2026-11-10-orchestrator-sessions.md
 */
export function CreateSessionOrchestratorField({
  orchestrator,
  setOrchestrator,
}: CreateSessionOrchestratorFieldProps) {
  return (
    <div>
      <label className="flex items-center gap-2 text-sm text-muted-foreground">
        <input
          data-testid="create-session-orchestrator-toggle"
          type="checkbox"
          className="h-4 w-4 rounded border-input"
          checked={orchestrator}
          onChange={(e) => setOrchestrator(e.target.checked)}
        />
        Session orchestrator
      </label>
    </div>
  );
}
