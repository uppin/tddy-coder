/**
 * Pure reducer for terminal control state.
 *
 * Folds TerminalControlEvent stream events (snapshot + live deltas) into a view
 * of whether the current screen is the controller of a session's terminals.
 */

export interface TerminalControlState {
  isController: boolean;
  holderScreenId: string;
}

export interface TerminalControlEventData {
  holderScreenId: string;
  youAreController: boolean;
}

export const initialTerminalControlState: TerminalControlState = {
  isController: false,
  holderScreenId: "",
};

export function applyTerminalControlEvent(
  _state: TerminalControlState,
  event: TerminalControlEventData,
): TerminalControlState {
  return {
    isController: event.youAreController,
    holderScreenId: event.holderScreenId,
  };
}
