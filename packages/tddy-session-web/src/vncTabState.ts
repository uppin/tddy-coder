/**
 * Pure reducer for VNC tab state.
 *
 * Tracks the list of VNC targets for a session, their streaming status,
 * and whether the credential vault is currently locked.
 */

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface VncTarget {
  id: string;
  label: string;
  host: string;
  port: number;
}

/** Per-target streaming status. */
export type VncStreamStatus =
  | "stopped"    // No bridge running for this target
  | "starting"   // StartVncStream RPC in flight
  | "streaming"  // Bridge running, video track published
  | "error";     // Bridge error or start failed

export interface VncTabState {
  /** Known VNC targets for this session (from ListVncTargets). */
  targets: VncTarget[];
  /** Streaming status per target id. */
  streamStatus: Record<string, VncStreamStatus>;
  /** Whether the vault is locked (requires UnlockVncVault before add/start). */
  isVaultLocked: boolean;
  /** Last error message, or null if no error. */
  error: string | null;
  /** The target id currently shown in the overlay (or null if closed). */
  activeOverlayTargetId: string | null;
}

export type VncTabAction =
  | { type: "set_targets"; targets: VncTarget[] }
  | { type: "add_target"; target: VncTarget }
  | { type: "remove_target"; targetId: string }
  | { type: "set_stream_status"; targetId: string; status: VncStreamStatus }
  | { type: "set_vault_locked"; locked: boolean }
  | { type: "set_error"; error: string | null }
  | { type: "open_overlay"; targetId: string }
  | { type: "close_overlay" };

// ---------------------------------------------------------------------------
// Initial state
// ---------------------------------------------------------------------------

export const initialVncTabState: VncTabState = {
  targets: [],
  streamStatus: {},
  isVaultLocked: true,
  error: null,
  activeOverlayTargetId: null,
};

// ---------------------------------------------------------------------------
// Reducer
// ---------------------------------------------------------------------------

/**
 * Apply a single VNC tab action to the state.
 *
 * Returns a new state object; never mutates the input.
 */
export function applyVncTabAction(
  state: VncTabState,
  action: VncTabAction,
): VncTabState {
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
