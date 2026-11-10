import type { ViewRect } from "./lib/terminalStatusBarLayout";
import type { TransitionCounters } from "./connection/terminalPresentation";

/** A view rectangle for layout tests (terminalStatusBarLayout / terminalPresentation suites). */
export function aViewRect(overrides: Partial<ViewRect> = {}): ViewRect {
  return {
    left: 0,
    top: 0,
    right: 100,
    bottom: 50,
    ...overrides,
  };
}

/** Transition counters for the terminalPresentation suite. */
export function aConnectCounter(overrides: Partial<TransitionCounters> = {}): TransitionCounters {
  return {
    connectSessionCalls: 0,
    resumeSessionCalls: 0,
    disconnectCalls: 0,
    ...overrides,
  };
}
