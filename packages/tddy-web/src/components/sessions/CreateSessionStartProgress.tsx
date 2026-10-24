import { StartPhase_Step } from "../../gen/session_pb";
import { formatAttachmentBytes } from "../../lib/attachmentBytes";
import type { StartProgressState } from "./startProgress";

/** How each step is named in the list, and in its row's test id. */
const STEP_ROWS: Partial<Record<StartPhase_Step, { name: string; label: string }>> = {
  [StartPhase_Step.CLONE]: { name: "clone", label: "Cloning repository" },
  [StartPhase_Step.WORKTREE]: { name: "worktree", label: "Worktree" },
  [StartPhase_Step.SEMANTIC_INDEX]: { name: "semantic-index", label: "Semantic index" },
  [StartPhase_Step.AGENT]: { name: "agent", label: "Agent" },
};

const STATE_MARK = { active: "…", done: "✓", failed: "✗" } as const;

export interface CreateSessionStartProgressProps {
  /** The host doing the work: the codebase host on a split start, the session's host otherwise. */
  hostLabel: string;
  progress: StartProgressState;
}

/**
 * The create pane's account of a streamed start: the steps the host has reported, in the order it
 * began them, each in progress, done or failed, and — while the clone runs — git's own stage, a
 * percent bar, the bytes received and the throughput. Renders nothing until a step is reported.
 *
 * Feature docs: docs/ft/web/session-drawer.md (Start progress)
 */
export function CreateSessionStartProgress({ hostLabel, progress }: CreateSessionStartProgressProps) {
  if (progress.steps.length === 0) return null;
  return (
    <div data-testid="create-session-start-progress" className="flex flex-col gap-1 text-sm">
      <p data-testid="create-session-start-progress-heading" className="font-medium">
        Creating session on {hostLabel}
      </p>
      <ul className="flex flex-col gap-1">
        {progress.steps.map(({ step, state }) => {
          const row = STEP_ROWS[step];
          if (!row) return null;
          return (
            <li
              key={row.name}
              data-testid={`create-session-start-step-${row.name}`}
              data-state={state}
              className={state === "failed" ? "text-destructive" : "text-muted-foreground"}
            >
              <span aria-hidden="true">{STATE_MARK[state]}</span> {row.label}
              {step === StartPhase_Step.CLONE && state === "active" && progress.clone ? (
                <CloneProgressRow clone={progress.clone} />
              ) : null}
            </li>
          );
        })}
      </ul>
    </div>
  );
}

function CloneProgressRow({ clone }: { clone: NonNullable<StartProgressState["clone"]> }) {
  const percent = Math.max(0, Math.min(100, clone.percent));
  return (
    <div data-testid="create-session-clone-progress" className="ml-5 flex flex-col gap-0.5">
      <span>
        {clone.stage} · {percent}%
      </span>
      <div
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={percent}
        className="h-1.5 w-full overflow-hidden rounded bg-muted"
      >
        <div className="h-full bg-primary" style={{ width: `${percent}%` }} />
      </div>
      <span>
        {formatAttachmentBytes(Number(clone.bytesReceived))} ·{" "}
        {formatAttachmentBytes(Number(clone.bytesPerSecond))}/s
      </span>
    </div>
  );
}
