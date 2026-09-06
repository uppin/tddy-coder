/**
 * Asks the operator for a key passphrase, on behalf of a host that is waiting.
 *
 * Server-initiated, unlike `ScreenSharingPassphraseDialog` — that one is the UI deciding to ask
 * before making a call. Here the host raised the question on `StreamHostPrompts` and is blocked
 * until an answer comes back.
 *
 * The dialog always names the host and shows its public-key fingerprint, so an operator can verify
 * out of band before handing over a secret. A **changed** key blocks submission entirely rather than
 * warning and proceeding.
 *
 * The encryption happens *here*, not in the caller: the plaintext then exists only inside this
 * component's state, and every path out of the dialog carries ciphertext. A caller handed the
 * passphrase would be one `console.log` away from undoing the whole node.
 */

import React, { useState } from "react";
import { encryptForHost } from "../../lib/encryptForHost";

export interface HostPassphraseDialogProps {
  hostId: string;
  /** What is being unlocked — e.g. the key file name. Never a secret. */
  subject: string;
  /** The host's public-key fingerprint, shown for out-of-band verification. */
  fingerprint: string;
  /** The host's published SPKI DER public key, which the answer is encrypted under. */
  spkiDer: Uint8Array;
  /** True when this fingerprint differs from the one pinned for this host. */
  keyChanged: boolean;
  /** Receives the RSA-OAEP ciphertext — the passphrase itself never leaves this component. */
  onSubmit: (encryptedAnswer: Uint8Array) => void;
  onCancel: () => void;
}

export function HostPassphraseDialog({
  hostId,
  subject,
  fingerprint,
  spkiDer,
  keyChanged,
  onSubmit,
  onCancel,
}: HostPassphraseDialogProps): React.ReactElement {
  const [passphrase, setPassphrase] = useState("");
  const [encrypting, setEncrypting] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);

  // Empty is refused because an empty answer only wastes the host's single-use prompt; `encrypting`
  // is refused because one prompt accepts exactly one answer.
  const blocked = keyChanged || encrypting || passphrase.length === 0;

  const handleSubmit = async (event: React.FormEvent) => {
    event.preventDefault();
    if (blocked) return;
    setEncrypting(true);
    setFailure(null);
    try {
      const encryptedAnswer = await encryptForHost(spkiDer, passphrase);
      setPassphrase("");
      onSubmit(encryptedAnswer);
    } catch (error) {
      // The answer never left, so say so rather than leaving the operator watching a dead dialog.
      // The field is still cleared: a secret on screen after a failure is the same secret on screen.
      setPassphrase("");
      setFailure(error instanceof Error ? error.message : String(error));
    } finally {
      setEncrypting(false);
    }
  };

  const handleCancel = () => {
    setPassphrase("");
    onCancel();
  };

  return (
    <div
      data-testid={`host-passphrase-dialog-${hostId}`}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
    >
      <form
        onSubmit={handleSubmit}
        className="bg-background border border-border rounded-md p-4 w-96 flex flex-col gap-3"
      >
        <p className="text-sm font-medium">Passphrase for {subject}</p>
        <p className="text-xs text-muted-foreground">
          {hostId} is waiting to unlock {subject} and load it into its ssh-agent.
        </p>
        <p className="text-xs text-muted-foreground break-all">
          Answer encrypted for host key <span className="font-mono">{fingerprint}</span>
        </p>
        {keyChanged && (
          <p
            data-testid="host-key-changed-warning"
            className="text-xs text-destructive border border-destructive rounded px-2 py-1"
          >
            This host&apos;s key has changed since you last answered a prompt from it. Verify the
            fingerprint above with the host before sending anything.
          </p>
        )}
        {failure !== null && (
          <p data-testid="host-passphrase-failure" className="text-xs text-destructive">
            Nothing was sent: {failure}
          </p>
        )}
        <input
          data-testid="host-passphrase-input"
          type="password"
          autoComplete="off"
          value={passphrase}
          disabled={keyChanged}
          onChange={(e) => setPassphrase(e.target.value)}
          className="border border-border rounded px-2 py-1 text-sm bg-background"
          placeholder="Passphrase"
        />
        <div className="flex gap-2 justify-end">
          <button
            type="button"
            data-testid="host-passphrase-cancel"
            onClick={handleCancel}
            className="px-3 py-1 text-xs border border-border rounded hover:bg-muted"
          >
            Cancel
          </button>
          <button
            type="submit"
            data-testid="host-passphrase-submit"
            disabled={blocked}
            className="px-3 py-1 text-xs bg-foreground text-background rounded hover:opacity-90 disabled:opacity-50"
          >
            Send
          </button>
        </div>
      </form>
    </div>
  );
}
