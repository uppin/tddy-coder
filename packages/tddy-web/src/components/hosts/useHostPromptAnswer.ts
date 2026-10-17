/**
 * Everything a surface needs to put a host's passphrase question in front of an operator and send
 * the answer back: the prompt feed, the key-continuity check keyed to the prompt in hand, and the
 * handlers `HostPassphraseDialog` is wired to.
 *
 * Two surfaces render the dialog from this — the Hosts screen's add-key row and the create-session
 * form's start — so the guarantees below hold for both by construction rather than by two copies
 * staying alike.
 *
 * The prompt feed is read only while `active`. The daemon replays whatever is still outstanding to a
 * subscriber that arrives late (`host_prompt_stream.rs`), so there is no window in which a caller can
 * miss the question its own call raised — and a screen nobody is acting on opens no streams at all.
 *
 * The key that arrives with a prompt is fingerprinted here before any dialog is shown, and it is
 * that derived value — never the fingerprint string the prompt advertises beside it — that is
 * displayed and pinned. The two fields ride the same unauthenticated channel and only one of them
 * encrypts anything; see `hostKeyPinning`.
 *
 * A checked key is held **together with the prompt it came from**, and only a pair whose prompt is
 * the one in hand is exposed. The fingerprint on screen is therefore always the digest of the key
 * that would do the encrypting: a second frame replacing the key retires the previous verdict in the
 * same render, and the dialog says nothing and sends nothing until the new key's check lands. Left as
 * two pieces of state, the frame in between shows the old key's fingerprint and its reassuring
 * verdict beside bytes that would encrypt for somebody else.
 *
 * Feature: `docs/ft/web/hosts-screen-add-key.md`
 */

import { useEffect, useState } from "react";
import { HostService } from "../../gen/host_pb";
import { useHostClient } from "../../rpc/connections/registry";
import { useHostPromptsFor, type HostPromptEventLike } from "../../rpc/useHostPrompts";
import {
  acceptChangedHostKey,
  verifyHostKey,
  type HostKeyCheck,
} from "../../lib/hostKeyPinning";

/**
 * A verified host key, kept with the prompt it arrived on.
 *
 * The pair is the point. `verifyHostKey` derives a fingerprint from one particular set of key bytes,
 * and everything the dialog says about a key — the fingerprint it shows, the continuity verdict it
 * blocks on, the key it encrypts under — has to be about *those* bytes. Held as one value, the two
 * cannot drift apart; held as two pieces of state, they do so on the first frame that replaces one.
 */
interface CheckedPromptKey {
  /** The very prompt whose key was checked — identity, not its id, is what binds the two. */
  prompt: HostPromptEventLike;
  check: HostKeyCheck;
}

export interface HostPromptAnswer {
  /** The question the host is waiting on, or `null` when it is asking nothing (or was answered). */
  prompt: HostPromptEventLike | null;
  /**
   * The checked key of the question being asked *now*, or `null` until its check lands — the one
   * value the dialog's fingerprint and continuity verdict are rendered from.
   */
  answering: CheckedPromptKey | null;
  /** Pin the key the host is presenting now, after the operator said the change is the host's own. */
  acceptChangedKey: () => void;
  /**
   * Send the ciphertext the dialog produced. The passphrase itself never reaches the caller.
   *
   * Resolves to what to tell the operator when the host did not take the answer, or `null` when it
   * did — the dialog is gone by then, so this is the only place a refusal can be read.
   */
  answer: (encryptedAnswer: Uint8Array) => Promise<string | null>;
  /** Close the question on this side only: the daemon has no withdraw call, so it expires on its own. */
  cancel: () => void;
}

/**
 * @param instanceId the host whose prompts are read and answered.
 * @param active whether anything is waiting on that host; `false` subscribes to nothing, and ends
 *   any question that was on screen.
 * @param sessionToken the operator's session, which the daemon resolves to the GitHub user a prompt
 *   was raised for. A parameter rather than read from the auth context because one caller, the
 *   create-session pane, is handed its token as a prop and is mounted without that context.
 */
export function useHostPromptAnswer(
  instanceId: string,
  active: boolean,
  sessionToken: string,
): HostPromptAnswer {
  const client = useHostClient(HostService, instanceId);
  const [handledPromptId, setHandledPromptId] = useState<string | null>(null);

  // Only while a call of the caller's is blocked on an answer: `null` subscribes to nothing.
  const feed = useHostPromptsFor(active ? instanceId : null, sessionToken);
  const prompt = feed && feed.promptId !== handledPromptId ? feed : null;

  // A new wait is not bound by what an earlier one closed.
  useEffect(() => {
    if (!active) setHandledPromptId(null);
  }, [active]);

  const [checkedKey, setCheckedKey] = useState<CheckedPromptKey | null>(null);

  // Compared here rather than cleared by an effect, and compared by the prompt's own identity rather
  // than by its id: an effect runs after the commit, so a frame that replaced the key would be
  // painted once with the previous key's fingerprint and its reassuring verdict beside bytes that
  // would encrypt for somebody else — the substitution the pin exists to catch, wearing the pin's
  // approval. Two frames can carry one `prompt_id`; only one of them is this object.
  const answering: CheckedPromptKey | null =
    checkedKey !== null && checkedKey.prompt === prompt ? checkedKey : null;

  // Checked once per question, after the commit rather than during render: the check *records* the
  // pin, and a render React discards would otherwise spend this host's one first-use trust slot on
  // a dialog nobody was ever shown.
  //
  // The key itself is checked, not the fingerprint string beside it. Both fields ride the same
  // unauthenticated channel, and only the key encrypts anything — so `verifyHostKey` derives a
  // fingerprint from the published key, pins that, and refuses a prompt whose two halves disagree.
  useEffect(() => {
    if (prompt === null) {
      setCheckedKey(null);
      return;
    }
    // Deriving a digest is asynchronous, so a prompt replaced while one is in flight would
    // otherwise land its verdict on the question that succeeded it.
    let current = true;
    const asked = prompt;
    void verifyHostKey(instanceId, asked.hostPublicKey, asked.hostPublicKeyFingerprint).then(
      (check) => {
        // Stored with the prompt it is about, so a verdict and the key it was derived from can only
        // ever be read as the pair they were produced as.
        if (current) setCheckedKey({ prompt: asked, check });
      },
    );
    return () => {
      current = false;
    };
  }, [instanceId, prompt]);

  /**
   * Pin the key the host is presenting now, after the operator said the change is the host's own.
   *
   * The verdict is settled here rather than re-derived: the check that produced it already hashed
   * this key, and asking storage again would only be able to agree with what was just written.
   */
  const acceptChangedKey = () => {
    if (answering === null || answering.check.fingerprint === null) return;
    acceptChangedHostKey(instanceId, answering.check.fingerprint);
    // Settled on the pair, so the accepted verdict stays bound to the key it was given for.
    setCheckedKey({
      prompt: answering.prompt,
      check: { ...answering.check, verdict: { kind: "unchanged" } },
    });
  };

  const answer = async (encryptedAnswer: Uint8Array): Promise<string | null> => {
    if (prompt === null || !client) return null;
    // One prompt accepts exactly one answer, so the question is closed here rather than when the
    // send comes back.
    setHandledPromptId(prompt.promptId);
    try {
      const response = await client.answerHostPrompt({
        sessionToken,
        daemonInstanceId: instanceId,
        promptId: prompt.promptId,
        encryptedAnswer,
      });
      if (response.accepted) return null;
      // The daemon's own words stand in: it distinguishes an unknown prompt from an expired one from
      // one already answered, and each sends the operator somewhere different.
      return response.rejectionReason === ""
        ? "The host refused that answer."
        : `The host refused that answer: ${response.rejectionReason}`;
    } catch (error) {
      return `The answer could not be sent: ${error instanceof Error ? error.message : String(error)}`;
    }
  };

  const cancel = () => {
    if (prompt !== null) setHandledPromptId(prompt.promptId);
  };

  return { prompt, answering, acceptChangedKey, answer, cancel };
}
