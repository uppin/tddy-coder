/**
 * What a host-prompt dialog says about the key its answer is encrypted under: the fingerprint, and
 * the continuity verdict's notice — with the one control a `changed` key allows, accepting it after
 * a deliberate two-step check.
 *
 * Shared by every dialog that answers a `StreamHostPrompts` question, so the pinning guarantees are
 * rendered by one component rather than by copies that drift.
 */

import React, { useState } from "react";
import type { KeyPinVerdict } from "../../lib/hostKeyPinning";

export interface HostKeyContinuityNoticesProps {
  hostId: string;
  /** Derived from the key that does the encrypting; `null` where none could be derived. */
  fingerprint: string | null;
  keyContinuity: KeyPinVerdict;
  onAcceptChangedKey: () => void;
}

export function HostKeyContinuityNotices({
  hostId,
  fingerprint,
  keyContinuity,
  onAcceptChangedKey,
}: HostKeyContinuityNoticesProps): React.ReactElement {
  const keyChanged = keyContinuity.kind === "changed";
  const keyUnverified = keyContinuity.kind === "unverified";
  const keyMismatched = keyContinuity.kind === "mismatched";
  const keyUnderivable = keyContinuity.kind === "underivable";
  // The key arrived and its digest is not back. Nothing is said about it and nothing may be sent to
  // it: a fingerprint shown now would be the *previous* key's, which is the one substitution the pin
  // exists to catch, and it would be wearing that key's approval.
  const keyUnchecked = keyContinuity.kind === "unchecked";
  const [changeVerified, setChangeVerified] = useState(false);

  return (
    <>
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
    </>
  );
}

/** Whether a verdict forbids sending anything to the key it is about. */
export function keyContinuityBlocks(keyContinuity: KeyPinVerdict): boolean {
  return (
    keyContinuity.kind === "changed" ||
    keyContinuity.kind === "mismatched" ||
    keyContinuity.kind === "underivable" ||
    keyContinuity.kind === "unchecked"
  );
}
