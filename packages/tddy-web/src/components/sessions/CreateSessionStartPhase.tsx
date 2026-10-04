import { StartPhase_Step } from "../../gen/session_pb";

/** What the create pane says while the host is in each slow step of a start. */
const STEP_TEXT: Record<StartPhase_Step, string> = {
  [StartPhase_Step.UNSPECIFIED]: "",
  [StartPhase_Step.WORKTREE]: "Creating worktree…",
  [StartPhase_Step.SEMANTIC_INDEX]: "Indexing (semantic)…",
  [StartPhase_Step.AGENT]: "Starting agent…",
};

export interface CreateSessionStartPhaseProps {
  /** The step the host last reported beginning, or `null` when none is under way. */
  step: StartPhase_Step | null;
}

/**
 * The line under the form naming the step of the start the host is in — the worktree, the semantic
 * index, the agent — while Create is disabled. Renders nothing between steps.
 *
 * Feature docs: docs/ft/web/session-drawer.md (Start progress) and docs/ft/web/session-code-pane.md (Indexing indicator)
 */
export function CreateSessionStartPhase({ step }: CreateSessionStartPhaseProps) {
  if (step === null || step === StartPhase_Step.UNSPECIFIED) return null;
  return (
    <p data-testid="create-session-start-phase" className="text-sm text-muted-foreground">
      {STEP_TEXT[step]}
    </p>
  );
}
