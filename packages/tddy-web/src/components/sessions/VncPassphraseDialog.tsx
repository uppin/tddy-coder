/**
 * Passphrase entry dialog for unlocking (or creating) the VNC credential vault.
 *
 * Shown the first time the user performs an operation that requires the vault key
 * (add a target with a password, or start a stream). On confirmation, calls
 * `UnlockVncVault` via the provided `onUnlock` callback.
 *
 * STUB — renders null.
 * Full implementation comes in the green phase.
 */

import React from "react";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface VncPassphraseDialogProps {
  /** Whether the dialog is visible. */
  open: boolean;
  /**
   * Whether a vault file already exists for this session.
   * When false, the dialog explains that this passphrase will encrypt all VNC passwords.
   * When true, it prompts the user to re-enter the existing passphrase.
   */
  vaultExists: boolean;
  /** Called with the entered passphrase when the user confirms. */
  onConfirm: (passphrase: string) => void;
  /** Called when the user cancels. */
  onCancel: () => void;
}

// ---------------------------------------------------------------------------
// Component (STUB)
// ---------------------------------------------------------------------------

export function VncPassphraseDialog(_props: VncPassphraseDialogProps): React.ReactElement | null {
  // FIXME: implement in green phase
  return null;
}
