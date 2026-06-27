/**
 * Pure reducer for screen sharing tab state.
 *
 * Tracks the list of screen-sharing targets for a session (VNC and RDP),
 * their streaming status, and whether the credential vault is currently locked.
 */

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface ScreenSharingTarget {
  id: string;
  label: string;
  host: string;
  port: number;
  protocol: "vnc" | "rdp";
}

export type ScreenSharingStreamStatus =
  | "stopped"    // No bridge running for this target
  | "starting"   // StartStream RPC in flight
  | "streaming"  // Bridge running, video track published
  | "error";     // Bridge error or start failed

export interface ScreenSharingTabState {
  targets: ScreenSharingTarget[];
  streamStatus: Record<string, ScreenSharingStreamStatus>;
  isVaultLocked: boolean;
  error: string | null;
  activeOverlayTargetId: string | null;
}

export type ScreenSharingTabAction =
  | { type: "set_targets"; targets: ScreenSharingTarget[] }
  | { type: "add_target"; target: ScreenSharingTarget }
  | { type: "remove_target"; targetId: string }
  | { type: "set_stream_status"; targetId: string; status: ScreenSharingStreamStatus }
  | { type: "set_vault_locked"; locked: boolean }
  | { type: "set_error"; error: string | null }
  | { type: "open_overlay"; targetId: string }
  | { type: "close_overlay" };

// ---------------------------------------------------------------------------
// Initial state
// ---------------------------------------------------------------------------

export const initialScreenSharingTabState: ScreenSharingTabState = {
  targets: [],
  streamStatus: {},
  isVaultLocked: true,
  error: null,
  activeOverlayTargetId: null,
};

// ---------------------------------------------------------------------------
// Reducer
// ---------------------------------------------------------------------------

export function applyScreenSharingTabAction(
  state: ScreenSharingTabState,
  action: ScreenSharingTabAction,
): ScreenSharingTabState {
  switch (action.type) {
    case "set_targets":
      return { ...state, targets: action.targets };

    case "add_target":
      return { ...state, targets: [...state.targets, action.target] };

    case "remove_target":
      return {
        ...state,
        targets: state.targets.filter((t) => t.id !== action.targetId),
      };

    case "set_stream_status":
      return {
        ...state,
        streamStatus: { ...state.streamStatus, [action.targetId]: action.status },
      };

    case "set_vault_locked":
      return { ...state, isVaultLocked: action.locked };

    case "set_error":
      return { ...state, error: action.error };

    case "open_overlay":
      return { ...state, activeOverlayTargetId: action.targetId };

    case "close_overlay":
      return { ...state, activeOverlayTargetId: null };
  }
}
