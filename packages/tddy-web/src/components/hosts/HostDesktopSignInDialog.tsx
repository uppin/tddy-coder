/**
 * Asks the operator to sign in to a host's desktop, shaped by the methods that desktop advertises
 * (`#host-desktops 8/8`).
 *
 * Answers a `DESKTOP_CREDENTIALS` question. The fields are the chosen method's and no others:
 *
 * | Method | Fields |
 * |---|---|
 * | VNC Authentication | password |
 * | VeNCrypt Plain / TLSPlain, Apple Remote Desktop, RDP TLS | username, password |
 * | RDP NLA | username, domain (optional), password |
 *
 * The strongest advertised method is preselected and the operator can switch to any other one the
 * desktop advertised. **Remember** asks the host to keep the credential in the operator's
 * credential vault; unticked, it is used once and kept nowhere.
 *
 * Built on `HostPassphraseDialog`'s guarantees: the host's key fingerprint is shown and pinned, a
 * changed key blocks the answer, and the answer leaves this component only as ciphertext — a
 * protobuf-encoded `DesktopCredentialAnswer`, RSA-OAEP-encrypted under the key the question
 * carried. A desktop sign-in never reuses the superseded `VncPassphraseDialog` / `vnc_*` surface.
 *
 * `error` is the desktop's own reason for refusing the last sign-in, shown when the host asks
 * again.
 */

import { create, toBinary } from "@bufbuild/protobuf";
import React, { useEffect, useState } from "react";
import {
  DesktopAuthMethod,
  DesktopCredentialAnswerSchema,
} from "../../gen/screen_sharing_pb";
import { encryptForHost } from "../../lib/encryptForHost";
import { acceptChangedHostKey, checkHostKey, type KeyPinVerdict } from "../../lib/hostKeyPinning";
import type { HostPromptEventLike } from "../../rpc/useHostPrompts";
import { HostKeyContinuityNotices, keyContinuityBlocks } from "./HostKeyContinuityNotices";

export interface HostDesktopSignInDialogProps {
  /** The `DESKTOP_CREDENTIALS` question being answered: its host, subject and key. */
  question: HostPromptEventLike;
  /** The methods the desktop advertises that tddy can sign in with, as `InspectHostDesktop` read them. */
  offered: DesktopAuthMethod[];
  /** Receives the RSA-OAEP ciphertext of the encoded `DesktopCredentialAnswer` — nothing else. */
  onSubmit: (answer: Uint8Array) => void;
  onCancel: () => void;
  /** Why the desktop refused the previous sign-in, in its own words. */
  error?: string;
}

/** What each method asks for, and what an operator calls it. */
const METHODS: Record<
  number,
  { label: string; username: boolean; domain: boolean; strength: number }
> = {
  [DesktopAuthMethod.RDP_NLA]: { label: "RDP (NLA)", username: true, domain: true, strength: 6 },
  [DesktopAuthMethod.APPLE_REMOTE_DESKTOP]: {
    label: "Apple Remote Desktop",
    username: true,
    domain: false,
    strength: 5,
  },
  [DesktopAuthMethod.VENCRYPT_TLS_PLAIN]: {
    label: "VeNCrypt (TLS, username and password)",
    username: true,
    domain: false,
    strength: 4,
  },
  [DesktopAuthMethod.VENCRYPT_PLAIN]: {
    label: "VeNCrypt (username and password)",
    username: true,
    domain: false,
    strength: 3,
  },
  [DesktopAuthMethod.RDP_TLS]: { label: "RDP (TLS)", username: true, domain: false, strength: 2 },
  [DesktopAuthMethod.VNC_PASSWORD]: {
    label: "VNC password",
    username: false,
    domain: false,
    strength: 1,
  },
};

/** The methods this dialog can sign in with, strongest first. */
function signInMethods(offered: DesktopAuthMethod[]): DesktopAuthMethod[] {
  return offered
    .filter((method) => method in METHODS)
    .sort((a, b) => METHODS[b].strength - METHODS[a].strength);
}

export function HostDesktopSignInDialog({
  question,
  offered,
  onSubmit,
  onCancel,
  error,
}: HostDesktopSignInDialogProps): React.ReactElement {
  const hostId = question.daemonInstanceId;
  const methods = signInMethods(offered);
  const [chosen, setChosen] = useState<DesktopAuthMethod | null>(null);
  // The strongest advertised method until the operator picks another; a pick the desktop no longer
  // advertises (a re-ask with a different list) falls back to the strongest.
  const method = chosen !== null && methods.includes(chosen) ? chosen : methods[0];
  const fields = METHODS[method];

  const [username, setUsername] = useState("");
  const [domain, setDomain] = useState("");
  const [password, setPassword] = useState("");
  const [remember, setRemember] = useState(false);
  const [encrypting, setEncrypting] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);

  // Checked once per question, after the commit rather than during render: the check records the
  // pin, and a render React discards would spend this host's one first-use trust slot on a dialog
  // nobody was shown. Held with the question it is about, so a verdict is never read beside another
  // question's key.
  //
  // The fingerprint pinned is the one the question advertised, as `HostDesktopOverlay`'s password
  // question always did; deriving it from `hostPublicKey` (`verifyHostKey`) is the stronger check
  // `HostAddKeyAction` makes.
  // TODO(host-desktops): docs/dev/todo/2026-10-21-desktop-sign-in-pins-the-advertised-fingerprint.md
  // — derive the fingerprint from the published key here too, so the pin cannot be satisfied by a
  // substituted key advertising the pinned string.
  const [checked, setChecked] = useState<{
    question: HostPromptEventLike;
    verdict: KeyPinVerdict;
  } | null>(null);
  useEffect(() => {
    setChecked({ question, verdict: checkHostKey(hostId, question.hostPublicKeyFingerprint) });
  }, [hostId, question]);
  const keyContinuity: KeyPinVerdict =
    checked !== null && checked.question === question ? checked.verdict : { kind: "unchecked" };

  const acceptChangedKey = () => {
    acceptChangedHostKey(hostId, question.hostPublicKeyFingerprint);
    setChecked({ question, verdict: { kind: "unchanged" } });
  };

  const keyBlocks = keyContinuityBlocks(keyContinuity);
  const blocked = keyBlocks || encrypting;

  const handleSubmit = async (event: React.FormEvent) => {
    event.preventDefault();
    if (blocked) return;
    setEncrypting(true);
    setFailure(null);
    try {
      const answer = create(DesktopCredentialAnswerSchema, {
        method,
        username: fields.username ? username : "",
        domain: fields.domain ? domain : "",
        password,
        remember,
      });
      const encrypted = await encryptForHost(
        question.hostPublicKey,
        toBinary(DesktopCredentialAnswerSchema, answer),
      );
      onSubmit(encrypted);
    } catch (e) {
      // The answer never left, so say so rather than leaving the operator watching a dead dialog.
      setFailure(e instanceof Error ? e.message : String(e));
    } finally {
      // A secret on screen after the answer left, or failed to, is the same secret on screen.
      setPassword("");
      setEncrypting(false);
    }
  };

  const handleCancel = () => {
    setPassword("");
    onCancel();
  };

  const inputClass = "border border-border rounded px-2 py-1 text-sm bg-background";

  return (
    <div
      data-testid={`host-desktop-sign-in-${hostId}`}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
    >
      <form
        onSubmit={handleSubmit}
        className="bg-background border border-border rounded-md p-4 w-96 flex flex-col gap-3"
      >
        <p className="text-sm font-medium">Sign in to {question.subject}</p>
        <p className="text-xs text-muted-foreground">
          {hostId} is waiting for this desktop&apos;s credentials to open it.
        </p>
        <HostKeyContinuityNotices
          hostId={hostId}
          fingerprint={question.hostPublicKeyFingerprint}
          keyContinuity={keyContinuity}
          onAcceptChangedKey={acceptChangedKey}
        />
        {error !== undefined && (
          <p data-testid="host-desktop-sign-in-error" className="text-xs text-destructive">
            {error}
          </p>
        )}
        {failure !== null && (
          <p data-testid="host-passphrase-failure" className="text-xs text-destructive">
            Nothing was sent: {failure}
          </p>
        )}
        <select
          data-testid="host-desktop-sign-in-method"
          aria-label="Sign-in method"
          value={String(method)}
          onChange={(e) => setChosen(Number(e.target.value) as DesktopAuthMethod)}
          className={inputClass}
        >
          {methods.map((m) => (
            <option key={m} value={String(m)}>
              {METHODS[m].label}
            </option>
          ))}
        </select>
        {fields.username && (
          <input
            data-testid="host-desktop-sign-in-username"
            type="text"
            autoComplete="off"
            value={username}
            disabled={keyBlocks}
            onChange={(e) => setUsername(e.target.value)}
            className={inputClass}
            placeholder="Username"
          />
        )}
        {fields.domain && (
          <input
            data-testid="host-desktop-sign-in-domain"
            type="text"
            autoComplete="off"
            value={domain}
            disabled={keyBlocks}
            onChange={(e) => setDomain(e.target.value)}
            className={inputClass}
            placeholder="Domain (optional)"
          />
        )}
        <input
          data-testid="host-desktop-sign-in-password"
          type="password"
          autoComplete="off"
          value={password}
          disabled={keyBlocks}
          onChange={(e) => setPassword(e.target.value)}
          className={inputClass}
          placeholder="Password"
        />
        <label className="text-xs flex items-center gap-2">
          <input
            data-testid="host-desktop-sign-in-remember"
            type="checkbox"
            checked={remember}
            onChange={(e) => setRemember(e.target.checked)}
          />
          <span>Remember this sign-in on {hostId}</span>
        </label>
        <div className="flex gap-2 justify-end">
          <button
            type="button"
            data-testid="host-desktop-sign-in-cancel"
            onClick={handleCancel}
            className="px-3 py-1 text-xs border border-border rounded hover:bg-muted"
          >
            Cancel
          </button>
          <button
            type="submit"
            data-testid="host-desktop-sign-in-submit"
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
