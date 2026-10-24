import {
  StartPhase_Boundary,
  type CloneProgress,
  StartPhase_Step,
  type StartSessionEvent,
} from "../../gen/session_pb";

/** One step of a start as the create pane lists it. */
export interface StartProgressStep {
  step: StartPhase_Step;
  state: "active" | "done" | "failed";
}

/**
 * What a streamed start has reported so far: its steps in the order the host began them, the latest
 * clone progress while the clone step runs, and whether the stream failed (the active step then
 * becomes `failed`).
 */
export interface StartProgressState {
  steps: StartProgressStep[];
  clone: CloneProgress | null;
}

export const NO_START_PROGRESS: StartProgressState = { steps: [], clone: null };

/** Fold one `StartSessionEvent` into the state. Attachment progress and keep-alives change nothing. */
export function startProgressAfter(
  state: StartProgressState,
  event: StartSessionEvent,
): StartProgressState {
  switch (event.event.case) {
    case "phase": {
      const { step, boundary } = event.event.value;
      if (boundary === StartPhase_Boundary.BEGIN) {
        const steps = state.steps.filter((entry) => entry.step !== step);
        return { ...state, steps: [...steps, { step, state: "active" }] };
      }
      if (boundary === StartPhase_Boundary.END) {
        return {
          // The clone's progress belongs to the clone step: it is over when that step is.
          clone: step === StartPhase_Step.CLONE ? null : state.clone,
          steps: state.steps.map((entry) =>
            entry.step === step ? { ...entry, state: "done" } : entry,
          ),
        };
      }
      return state;
    }
    case "cloneProgress":
      return { ...state, clone: event.event.value };
    default:
      return state;
  }
}

/** The state after the stream failed: the step in progress is marked failed. */
export function startProgressFailed(state: StartProgressState): StartProgressState {
  return {
    ...state,
    steps: state.steps.map((entry) =>
      entry.state === "active" ? { ...entry, state: "failed" } : entry,
    ),
  };
}
