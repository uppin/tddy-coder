/**
 * Passphrase entry dialog for unlocking (or creating) the screen-sharing credential vault.
 *
 * Shown the first time the user performs an operation that requires the vault key
 * (add a target with a password, or start a stream). On confirmation, calls
 * `UnlockVault` via the provided `onConfirm` callback.
 */

import React, { useState } from "react";

export interface ScreenSharingPassphraseDialogProps {
  open: boolean;
  vaultExists: boolean;
  onConfirm: (passphrase: string) => void;
  onCancel: () => void;
}

export function ScreenSharingPassphraseDialog({
  open,
  vaultExists,
  onConfirm,
  onCancel,
}: ScreenSharingPassphraseDialogProps): React.ReactElement | null {
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
      data-testid="sessions-screen-sharing-passphrase-dialog"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
    >
      <div className="bg-background border border-border rounded-md p-4 w-80 flex flex-col gap-3">
        <p className="text-sm font-medium">
          {vaultExists ? "Enter vault passphrase" : "Set vault passphrase"}
        </p>
        <p className="text-xs text-muted-foreground">
          {vaultExists
            ? "Enter your passphrase to unlock the screen-sharing credential vault."
            : "This passphrase will encrypt your screen-sharing passwords."}
        </p>
        <input
          data-testid="sessions-screen-sharing-passphrase-input"
          type="password"
          value={passphrase}
          onChange={(e) => setPassphrase(e.target.value)}
          className="border border-border rounded px-2 py-1 text-sm bg-background"
          placeholder="Passphrase"
        />
        <div className="flex gap-2 justify-end">
          <button
            data-testid="sessions-screen-sharing-passphrase-cancel"
            onClick={handleCancel}
            className="px-3 py-1 text-xs border border-border rounded hover:bg-muted"
          >
            Cancel
          </button>
          <button
            data-testid="sessions-screen-sharing-passphrase-confirm"
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
