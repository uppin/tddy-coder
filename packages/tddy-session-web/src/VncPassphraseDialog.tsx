/**
 * Passphrase entry dialog for unlocking (or creating) the VNC credential vault.
 *
 * Shown the first time the user performs an operation that requires the vault key
 * (add a target with a password, or start a stream). On confirmation, calls
 * `UnlockVncVault` via the provided `onConfirm` callback.
 */

import React, { useState } from "react";

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
// Component
// ---------------------------------------------------------------------------

export function VncPassphraseDialog({ open, vaultExists, onConfirm, onCancel }: VncPassphraseDialogProps): React.ReactElement | null {
  const [passphrase, setPassphrase] = useState("");

  if (!open) return null;

  const handleConfirm = () => {
    onConfirm(passphrase);
    setPassphrase("");
  };

  const handleCancel = () => {
    setPassphrase("");
    onCancel();
  };

  return (
    <div
      data-testid="sessions-vnc-passphrase-dialog"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
    >
      <div className="bg-background border border-border rounded-md p-4 w-80 flex flex-col gap-3">
        <p className="text-sm font-medium">
          {vaultExists ? "Enter vault passphrase" : "Set vault passphrase"}
        </p>
        <p className="text-xs text-muted-foreground">
          {vaultExists
            ? "Enter your passphrase to unlock the VNC credential vault."
            : "This passphrase will encrypt your VNC passwords."}
        </p>
        <input
          data-testid="sessions-vnc-passphrase-input"
          type="password"
          value={passphrase}
          onChange={(e) => setPassphrase(e.target.value)}
          className="border border-border rounded px-2 py-1 text-sm bg-background"
          placeholder="Passphrase"
        />
        <div className="flex gap-2 justify-end">
          <button
            onClick={handleCancel}
            className="px-3 py-1 text-xs border border-border rounded hover:bg-muted"
          >
            Cancel
          </button>
          <button
            data-testid="sessions-vnc-passphrase-confirm"
            onClick={handleConfirm}
            className="px-3 py-1 text-xs bg-foreground text-background rounded hover:opacity-90"
          >
            Confirm
          </button>
        </div>
      </div>
    </div>
  );
}
