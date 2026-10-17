/**
 * The passphrase question a host raises while a session start is cloning, shown over the
 * create-session form.
 *
 * A start that must clone its project blocks on the host's ssh key passphrase, and the form is the
 * only surface the operator is looking at. The question, its key-continuity check and its answer are
 * the Hosts screen's (`useHostPromptAnswer`); this only decides when to listen — while a start is in
 * flight, against the host that is cloning — and renders nothing the rest of the time, so a form
 * nobody is submitting opens no stream.
 *
 * PRD: docs/ft/daemon/1-WIP/PRD-2026-10-17-clone-passphrase-prompt.md (AC12)
 */

import React from "react";
import { HostPassphraseDialog } from "../hosts/HostPassphraseDialog";
import { useHostPromptAnswer } from "../hosts/useHostPromptAnswer";

export interface StartPassphrasePromptProps {
  /** The daemon that is cloning — the one whose prompt feed is read and whose prompts are answered. */
  instanceId: string;
  /** Whether a start is in flight. */
  active: boolean;
  /** The operator's session — the one the start was made with, so the prompt is theirs to answer. */
  sessionToken: string;
}

export function StartPassphrasePrompt({
  instanceId,
  active,
  sessionToken,
}: StartPassphrasePromptProps): React.ReactElement | null {
  const { prompt, answering, acceptChangedKey, answer, cancel } = useHostPromptAnswer(
    instanceId,
    active,
    sessionToken,
  );
  if (prompt === null) return null;
  return (
    <HostPassphraseDialog
      hostId={instanceId}
      subject={prompt.subject}
      // Only what has been established about the key in hand: until its check lands there is nothing
      // to say and nothing to send.
      fingerprint={answering?.check.fingerprint ?? null}
      spkiDer={prompt.hostPublicKey}
      keyContinuity={answering?.check.verdict ?? { kind: "unchecked" }}
      // A refusal has no form field to land in here: the start is still waiting on this very host and
      // reports its own failure when the prompt expires unanswered.
      onSubmit={(encryptedAnswer) => void answer(encryptedAnswer)}
      onAcceptChangedKey={acceptChangedKey}
      onCancel={cancel}
    />
  );
}
