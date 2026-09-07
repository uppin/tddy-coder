/**
 * Asks the operator for a key passphrase, on behalf of a host that is waiting.
 *
 * Server-initiated, unlike `ScreenSharingPassphraseDialog` — that one is the UI deciding to ask
 * before making a call. Here the host raised the question on `StreamHostPrompts` and is blocked
 * until an answer comes back.
 *
 * The dialog always names the host and shows its public-key fingerprint — the one derived from the
 * key that will do the encrypting, which is what its caller passes — so an operator can verify out
 * of band before handing over a secret. A **changed** key blocks submission entirely rather than
 * warning and proceeding, and is released only by an operator who says, in two deliberate steps,
 * that they checked the new key with the host itself.
 *
 * The encryption happens *here*, not in the caller: the plaintext then exists only inside this
 * component's state, and every path out of the dialog carries ciphertext. A caller handed the
 * passphrase would be one `console.log` away from undoing the whole node.
 */

import React, { useState } from "react";
import { encryptForHost } from "../../lib/encryptForHost";
import type { KeyPinVerdict } from "../../lib/hostKeyPinning";

export interface HostPassphraseDialogProps {
  hostId: string;
  /** What is being unlocked — e.g. the key file name. Never a secret. */
  subject: string;
  /**
   * The host's public-key fingerprint, shown for out-of-band verification.
   *
   * Derived by the caller from {@link HostPassphraseDialogProps.spkiDer}, never the string a prompt
   * advertised beside it: an operator comparing this against the host is comparing the key their
   * passphrase is encrypted under. `null` where none could be derived, which is a state that blocks
   * anyway — see `underivable` on the verdict.
   */
  fingerprint: string | null;
  /** The host's published SPKI DER public key, which the answer is encrypted under. */
  spkiDer: Uint8Array;
  /**
   * What the continuity check concluded about this host's key — the one thing the dialog is told
   * about the pin, and both what it blocks on and what it says.
   *
   * A separate `keyChanged` boolean stood beside this verdict while the verdict had no reader. Two
   * props encoding the same fact can disagree, and the arm that would have gone unsaid is exactly
   * the one worth saying: `unverified` means no conclusion was available at all — no key was
   * presented, or storage refused to be read — so the sighting is evidence of nothing, in either
   * direction. Rendering it as an ordinary first sighting would let an active substitution look
   * routine, which is precisely what the pin exists to prevent.
   *
   * `changed`, `mismatched`, `underivable` and `unchecked` block the answer. `unverified` warns and
   * does **not**: a host that cannot be pin-checked has not been caught doing anything, and refusing
   * here would make the feature unusable in any browser that will not store a pin.
   */
  keyContinuity: KeyPinVerdict;
  /** Receives the RSA-OAEP ciphertext — the passphrase itself never leaves this component. */
  onSubmit: (encryptedAnswer: Uint8Array) => void;
  /**
   * The operator has verified the changed key with the host and accepts it as the new pin.
   *
   * Offered for `changed` alone. A rotated host key would otherwise lock the operator out of their
   * own host permanently — the daemon regenerating `host-prompt-key.pem` is enough to cause it —
   * with no remedy short of clearing browser storage. `mismatched` gets no such path: a frame whose
   * two halves contradict each other is not a key anybody can choose to trust.
   */
  onAcceptChangedKey: () => void;
  onCancel: () => void;
}

export function HostPassphraseDialog({
  hostId,
  subject,
  fingerprint,
  spkiDer,
  keyContinuity,
  onSubmit,
  onAcceptChangedKey,
  onCancel,
}: HostPassphraseDialogProps): React.ReactElement {
  const keyChanged = keyContinuity.kind === "changed";
  const keyUnverified = keyContinuity.kind === "unverified";
  const keyMismatched = keyContinuity.kind === "mismatched";
  const keyUnderivable = keyContinuity.kind === "underivable";
  // The key arrived and its digest is not back. Nothing is said about it and nothing may be sent to
  // it: a fingerprint shown now would be the *previous* key's, which is the one substitution the pin
  // exists to catch, and it would be wearing that key's approval.
  const keyUnchecked = keyContinuity.kind === "unchecked";
  const [passphrase, setPassphrase] = useState("");
  const [encrypting, setEncrypting] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [changeVerified, setChangeVerified] = useState(false);

  // Empty is refused because an empty answer only wastes the host's single-use prompt; `encrypting`
  // is refused because one prompt accepts exactly one answer. Each blocking key state is refused for
  // a reason of its own, spelled out beside its notice below.
  const keyBlocks = keyChanged || keyMismatched || keyUnderivable || keyUnchecked;
  const blocked = keyBlocks || encrypting || passphrase.length === 0;

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
        {fingerprint !== null && !keyMismatched && (
          <p className="text-xs text-muted-foreground break-all">
            Answer encrypted for host key <span className="font-mono">{fingerprint}</span>
          </p>
        )}
        {keyChanged && (
          <>
            <p
              data-testid="host-key-changed-warning"
              className="text-xs text-destructive border border-destructive rounded px-2 py-1"
            >
              This host&apos;s key has changed since you last answered a prompt from it. Verify the
              fingerprint above with the host before sending anything.
            </p>
            <div className="flex flex-col gap-1 border border-border rounded px-2 py-1">
              <label className="text-xs flex items-start gap-2">
                <input
                  data-testid="host-key-accept-confirm"
                  type="checkbox"
                  checked={changeVerified}
                  onChange={(e) => setChangeVerified(e.target.checked)}
                />
                <span>
                  I checked the fingerprint above with {hostId} itself, and its key changed because
                  the host changed it.
                </span>
              </label>
              <button
                type="button"
                data-testid="host-key-accept-submit"
                disabled={!changeVerified}
                onClick={onAcceptChangedKey}
                className="self-end px-3 py-1 text-xs border border-destructive rounded hover:bg-muted disabled:opacity-50"
              >
                Accept this key for {hostId}
              </button>
            </div>
          </>
        )}
        {keyMismatched && (
          <p
            data-testid="host-key-mismatch-warning"
            className="text-xs text-destructive border border-destructive rounded px-2 py-1"
          >
            This prompt contradicts itself: it advertises host key{" "}
            <span className="font-mono break-all">{keyContinuity.advertisedFingerprint}</span>, but
            the key it carries is{" "}
            <span className="font-mono break-all">{keyContinuity.derivedFingerprint}</span>. A host
            describing its own key gets it right, so something rewrote this in flight. Nothing can be
            sent to it.
          </p>
        )}
        {keyUnderivable && (
          <p
            data-testid="host-key-underivable-notice"
            className="text-xs text-destructive border border-destructive rounded px-2 py-1"
          >
            This host&apos;s key cannot be checked or encrypted to from this page:{" "}
            {keyContinuity.reason}. Nothing has been sent.
          </p>
        )}
        {keyUnchecked && (
          <p
            data-testid="host-key-unchecked-notice"
            className="text-xs text-muted-foreground border border-border rounded px-2 py-1"
          >
            Checking the key this host is presenting. Nothing can be sent until it is checked.
          </p>
        )}
        {keyUnverified && (
          <p
            data-testid="host-key-unverified-notice"
            className="text-xs text-muted-foreground border border-border rounded px-2 py-1"
          >
            Could not check whether this host&apos;s key has changed since last time. Verify the
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
          disabled={keyBlocks}
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
