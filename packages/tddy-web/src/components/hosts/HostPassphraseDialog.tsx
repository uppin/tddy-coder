/**
 * Asks the operator for a secret, on behalf of a host that is waiting on it.
 *
 * One dialog for every question on `StreamHostPrompts` — a key passphrase (`#hosts-screen 6/8`) and
 * a desktop password (`#hosts-screen 8/8`) — because what has to be got right is the same for both:
 * the fingerprint is shown, a changed key blocks the answer, and the plaintext never leaves this
 * component. Only the words differ, and they are the caller's to supply.
 *
 * Server-initiated: the host raised the question on `StreamHostPrompts` and is blocked until an
 * answer comes back. It is the last place in this app that asks a person for a secret before a
 * call — the session screen-sharing form no longer does, because a session's own key opens the
 * credential store the desktop password lives in.
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
import { HostKeyContinuityNotices, keyContinuityBlocks } from "./HostKeyContinuityNotices";

/**
 * The words one question is asked in.
 *
 * The dialog is the same on both of this feed's flows — a host is waiting, and the answer leaves as
 * ciphertext — but what it is waiting *for* is not, and an operator told a desktop password is going
 * into an ssh-agent has been told something untrue.
 */
export interface HostPromptWording {
  /** The line the dialog leads with. */
  title: string;
  /** One sentence saying what the host will do with the answer. */
  explanation: string;
  /** What the operator is being asked to type. */
  placeholder: string;
}

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
  /**
   * The words to ask in. Defaults to the key passphrase this dialog was built for
   * (`#hosts-screen 6/8`), so a caller answering that question says nothing.
   */
  wording?: HostPromptWording;
  /** Receives the RSA-OAEP ciphertext — the answer itself never leaves this component. */
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
  wording,
  onSubmit,
  onAcceptChangedKey,
  onCancel,
}: HostPassphraseDialogProps): React.ReactElement {
  const words = wording ?? {
    title: `Passphrase for ${subject}`,
    explanation: `${hostId} is waiting to unlock ${subject} and load it into its ssh-agent.`,
    placeholder: "Passphrase",
  };
  const [passphrase, setPassphrase] = useState("");
  const [encrypting, setEncrypting] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);

  // Empty is refused because an empty answer only wastes the host's single-use prompt: an empty
  // passphrase unlocks no encrypted key. (A desktop with no password is answered by
  // HostDesktopSignInDialog, which does not go through this dialog.) `encrypting` is refused because one prompt accepts exactly one answer. Each blocking key state
  // is refused for a reason of its own, spelled out beside its notice below.
  const keyBlocks = keyContinuityBlocks(keyContinuity);
  const answerMissing = passphrase.length === 0;
  const blocked = keyBlocks || encrypting || answerMissing;

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
        <p className="text-sm font-medium">{words.title}</p>
        <p className="text-xs text-muted-foreground">{words.explanation}</p>
        <HostKeyContinuityNotices
          hostId={hostId}
          fingerprint={fingerprint}
          keyContinuity={keyContinuity}
          onAcceptChangedKey={onAcceptChangedKey}
        />
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
          placeholder={words.placeholder}
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
