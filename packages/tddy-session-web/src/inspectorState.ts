// Inspector panel state machine for the session inspector drawer.

export type InspectorState = { open: boolean; expanded: boolean };

export type InspectorAction =
  | { type: "open" }
  | { type: "close" }
  | { type: "toggle" }
  | { type: "expand" }
  | { type: "restore" }
  | { type: "select" };

/**
 * Returns the default open state when a session is selected: closed, whatever the session's
 * liveness. An active session shows its terminal and an inactive one shows its recorded activities,
 * so neither has a reason to have the drawer opened for it — the operator asks for it via the
 * `Inspector` toggle. See docs/ft/web/inactive-session-activities.md § Inspector.
 */
export function defaultInspectorOpen(): boolean {
  return false;
}

/**
 * Pure reducer for inspector panel state transitions.
 */
export function nextInspectorState(
  state: InspectorState,
  action: InspectorAction,
): InspectorState {
  switch (action.type) {
    case "open":
      return { open: true, expanded: false };
    case "close":
      return { open: false, expanded: false };
    case "toggle":
      return { open: !state.open, expanded: false };
    case "expand":
      return { open: true, expanded: true };
    case "restore":
      return { open: true, expanded: false };
    case "select":
      return { open: defaultInspectorOpen(), expanded: false };
  }
}
