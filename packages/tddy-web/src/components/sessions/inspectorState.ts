// Inspector panel state machine for the session inspector drawer.

export type InspectorState = { open: boolean; expanded: boolean };

export type InspectorAction =
  | { type: "open" }
  | { type: "close" }
  | { type: "toggle" }
  | { type: "expand" }
  | { type: "restore" }
  | { type: "select"; isActive: boolean };

/**
 * Returns the default open state when a session is selected.
 * Active (connected) sessions hide the inspector; inactive sessions show it.
 */
export function defaultInspectorOpen(isActive: boolean): boolean {
  return !isActive;
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
      return { open: defaultInspectorOpen(action.isActive), expanded: false };
  }
}
