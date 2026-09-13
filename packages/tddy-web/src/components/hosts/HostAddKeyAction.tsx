/**
 * Load a private key into a host's ssh-agent, from the browser.
 *
 * This is the surface that *starts* the flow the rest of this node builds. `AddHostKey` blocks for
 * as long as the add takes: it raises a passphrase prompt on `StreamHostPrompts`, waits for the
 * ciphertext to come back on `AnswerHostPrompt`, unlocks the key and hands it to the agent. So this
 * component both issues that call and, through `useHostPrompts`, renders the dialog the same call
 * is waiting on.
 *
 * It is offered **only where there is an agent to add to**. A host whose agent did not answer needs
 * an agent started, not a key loaded, and offering an add there would put a control in front of an
 * operator that cannot do anything. `HostRowSshAgent` already draws that distinction for the
 * summary text; this reads the same block rather than restating the rule.
 *
 * The outcome is reported from `AddHostKeyOutcome`, not from `added` alone. A wrong passphrase is
 * worth retyping, an absent agent is not, and an expired prompt means answering faster — three
 * different next actions, which is the entire reason the response carries an enum.
 *
 * The key that arrives with a prompt is fingerprinted here before any dialog is shown, and it is
 * that derived value — never the fingerprint string the prompt advertises beside it — that is
 * displayed and pinned. The two fields ride the same unauthenticated channel and only one of them
 * encrypts anything; see `hostKeyPinning`.
 *
 * A checked key is held **together with the prompt it came from**, and only a pair whose prompt is
 * the one in hand is shown. The fingerprint on screen is therefore always the digest of the key that
 * would do the encrypting: a second frame replacing the key retires the previous verdict in the same
 * render, and the dialog says nothing and sends nothing until the new key's check lands. Left as two
 * pieces of state, the frame in between shows the old key's fingerprint and its reassuring verdict
 * beside bytes that would encrypt for somebody else.
 *
 * The key is named by an **absolute path**, in the picker and in the field alike. `~` is expanded
 * nowhere — not here, which does not know the host's home, and not in the daemon, whose confinement
 * is deliberately lexical — and its refusal (`KEY_OUTSIDE_HOME`) names no path, so an operator who
 * sent one would be told only that their key must be inside a home it already was inside. This side
 * holds the context that makes that legible, so this side refuses it.
 *
 * The prompt feed is read only while an add is in flight. The daemon replays whatever is still
 * outstanding to a subscriber that arrives late (`host_prompt_stream.rs`), so there is no window in
 * which this component can miss the question its own call raised — and a screenful of hosts nobody
 * is adding a key to opens no streams at all.
 *
 * Feature: `docs/ft/web/hosts-screen-add-key.md`
 */

import React, { useEffect, useRef, useState } from "react";
import {
  AddHostKeyOutcome,
  HostService,
  ProbeOutcome,
  type AddHostKeyResponse,
  type HostKeyCandidate,
  type HostSshAgent,
} from "../../gen/host_pb";
import { useHostClient } from "../../rpc/connections/registry";
import { useHostPrompts, type HostPromptEventLike } from "../../rpc/useHostPrompts";
import { useAuthContext } from "../../hooks/authProvider";
import {
  acceptChangedHostKey,
  verifyHostKey,
  type HostKeyCheck,
} from "../../lib/hostKeyPinning";
import { HostPassphraseDialog } from "./HostPassphraseDialog";

export interface HostAddKeyActionProps {
  instanceId: string;
  /** This host's ssh-agent probe result — the same block `HostRowSshAgent` summarises. */
  sshAgent: HostSshAgent | undefined;
}

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

/** What the add came to, as an operator reads it. */
interface AddReport {
  text: string;
  /** The daemon's own words, when it had any — detail, never the distinction itself. */
  title?: string;
}

/**
 * Whether an ssh-agent answered for this host, and there is therefore something to add a key to.
 *
 * Only `OK` licenses a finding, for the reason `HostRowSshAgent` spells out: proto3 enums are open,
 * so a probe outcome this bundle cannot name must not be read as a reachable agent. An agent
 * holding keys already is still an agent worth adding to — a host commonly needs a second key.
 */
function anAgentAnswered(sshAgent: HostSshAgent | undefined): boolean {
  return sshAgent !== undefined && sshAgent.outcome === ProbeOutcome.OK && sshAgent.reachable;
}

/**
 * What `outcome` means for the operator standing in front of it.
 *
 * Written per arm rather than by echoing `failureReason`, because the response carries an enum
 * precisely so that a daemon with nothing to say still distinguishes the three failures an operator
 * would act on differently. `failureReason` is offered as detail — except on the arm that has no
 * meaning of its own, where it is the only account there is.
 */
function reportOf(response: AddHostKeyResponse): AddReport {
  const detail = response.failureReason === "" ? undefined : response.failureReason;
  switch (response.outcome) {
    case AddHostKeyOutcome.ADDED:
      return { text: "Key added.", title: detail };
    case AddHostKeyOutcome.WRONG_PASSPHRASE:
      return { text: "That passphrase did not unlock the key. Try again.", title: detail };
    case AddHostKeyOutcome.PROMPT_EXPIRED:
      return {
        text: "The question expired before it was answered. Start the add again.",
        title: detail,
      };
    case AddHostKeyOutcome.NO_AGENT:
      return {
        text: "No ssh-agent answered on this host, so there was nothing to load the key into.",
        title: detail,
      };
    case AddHostKeyOutcome.KEY_UNREADABLE:
      return { text: "That key could not be read on this host.", title: detail };
    // UNSPECIFIED — the daemon's own fallback, for an answer it could not decrypt or an agent that
    // refused — and every outcome a newer daemon may add that this bundle cannot name. Neither has
    // a cause this bundle can state, so the daemon's words stand in rather than an invented one.
    default:
      return { text: detail ?? "The key was not added.", title: undefined };
  }
}

export function HostAddKeyAction({
  instanceId,
  sshAgent,
}: HostAddKeyActionProps): React.ReactElement | null {
  const client = useHostClient(HostService, instanceId);
  const { sessionToken } = useAuthContext();
  const [subject, setSubject] = useState("");
  const [adding, setAdding] = useState(false);
  // The in-flight `AddHostKey`, so cancelling the dialog can end it. The call blocks for the
  // prompt's whole lifetime, and a cancel that only closed the dialog would leave the row disabled
  // for minutes with nothing on screen explaining the wait.
  const addInFlight = useRef<AbortController | null>(null);
  const [report, setReport] = useState<AddReport | null>(null);
  const [addedFingerprint, setAddedFingerprint] = useState<string | null>(null);
  const [handledPromptId, setHandledPromptId] = useState<string | null>(null);

  // Only while a call of ours is blocked on an answer: `null` subscribes to nothing, so a row
  // nobody is adding a key to costs no stream.
  const prompt = useHostPrompts(adding ? instanceId : null);
  const outstandingPrompt = prompt && prompt.promptId !== handledPromptId ? prompt : null;

  const [checkedKey, setCheckedKey] = useState<CheckedPromptKey | null>(null);

  // The checked key of the question being asked *now*, or nothing — the one value the dialog is
  // rendered from.
  //
  // Compared here rather than cleared by an effect, and compared by the prompt's own identity rather
  // than by its id: an effect runs after the commit, so a frame that replaced the key would be
  // painted once with the previous key's fingerprint and its reassuring verdict beside bytes that
  // would encrypt for somebody else — the substitution the pin exists to catch, wearing the pin's
  // approval. Two frames can carry one `prompt_id`; only one of them is this object.
  const answering: CheckedPromptKey | null =
    checkedKey !== null && checkedKey.prompt === outstandingPrompt ? checkedKey : null;

  // Checked once per question, after the commit rather than during render: the check *records* the
  // pin, and a render React discards would otherwise spend this host's one first-use trust slot on
  // a dialog nobody was ever shown.
  //
  // The key itself is checked, not the fingerprint string beside it. Both fields ride the same
  // unauthenticated channel, and only the key encrypts anything — so `verifyHostKey` derives a
  // fingerprint from the published key, pins that, and refuses a prompt whose two halves disagree.
  useEffect(() => {
    if (outstandingPrompt === null) {
      setCheckedKey(null);
      return;
    }
    // Deriving a digest is asynchronous, so a prompt replaced while one is in flight would
    // otherwise land its verdict on the question that succeeded it.
    let current = true;
    const asked = outstandingPrompt;
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
  }, [instanceId, outstandingPrompt]);

  const [candidates, setCandidates] = useState<HostKeyCandidate[]>([]);
  const thereIsAnAgent = anAgentAnswered(sshAgent);

  // Asked once, and only of a host there is an agent to add to: a row that cannot take a key has no
  // business enumerating its operator's keys either. Addressed to this row's own host, because the
  // paths are files on one machine and a listing addressed to nobody in particular would offer
  // paths from whichever daemon took the call.
  useEffect(() => {
    if (!client || !thereIsAnAgent) return;
    let current = true;
    void client
      .listHostKeyCandidates({ sessionToken: sessionToken ?? "", daemonInstanceId: instanceId })
      .then((listed) => {
        if (current) setCandidates(listed.candidates);
      })
      .catch((error: unknown) => {
        // Not the operator's problem: a host that cannot say what keys it has — an older daemon, a
        // refusal — is not a host that cannot take one, and the field beside the picker still works.
        console.debug("[HostAddKeyAction] host offered no key listing", instanceId, error);
      });
    return () => {
      current = false;
    };
  }, [client, thereIsAnAgent, instanceId, sessionToken]);

  if (!thereIsAnAgent) {
    return null;
  }

  // One subject, two ways to name it: picking sets it, typing replaces it, and a typed path that is
  // none of the offered ones leaves the picker showing nothing rather than a choice nobody made.
  const pickedPath = candidates.some((candidate) => candidate.path === subject) ? subject : "";

  const startAdd = async () => {
    if (!isAnAbsolutePath(subject)) {
      // Refused here rather than sent: `confined_to_home` rejects this, and its refusal names no
      // path on purpose, so an operator who typed `~/.ssh/id_ed25519` would be told their key must
      // be inside their home — about a path that was.
      setReport({ text: ABSOLUTE_PATH_REQUIRED });
      return;
    }
    if (!client) {
      // Not a state to paper over: the row can name this host, but nothing routes to it, so the add
      // was never issued and must not read as one that failed on the host.
      setReport({ text: "This host cannot be reached from here." });
      return;
    }
    const call = new AbortController();
    addInFlight.current = call;
    setAdding(true);
    setReport(null);
    setAddedFingerprint(null);
    setHandledPromptId(null);
    try {
      const response = await client.addHostKey(
        {
          sessionToken: sessionToken ?? "",
          daemonInstanceId: instanceId,
          subject,
        },
        { signal: call.signal },
      );
      setReport(reportOf(response));
      // The fingerprint the agent reported, so the operator can match it against the key list
      // beside it rather than taking "done" on trust.
      setAddedFingerprint(response.added ? response.fingerprint : null);
    } catch (error) {
      // A call we aborted ourselves is not a failure to report: the operator already knows, because
      // they are the one who cancelled, and `cancelAdd` has said so.
      if (!call.signal.aborted) {
        setReport({ text: `The add could not be sent: ${messageOf(error)}` });
      }
    } finally {
      setAdding(false);
    }
  };

  /**
   * Give up on the add the operator started.
   *
   * The daemon has no withdraw call, so the prompt it raised stands until it expires on its own —
   * unanswered, which is a state it already handles. What ends here is this browser's side: the call
   * is aborted and the control comes back, rather than staying disabled for the prompt's full TTL.
   */
  const cancelAdd = () => {
    if (outstandingPrompt !== null) setHandledPromptId(outstandingPrompt.promptId);
    addInFlight.current?.abort();
    addInFlight.current = null;
    // Not left to the aborted call's `finally`: the row must come back now, whether or not the
    // transport surfaces the abort as a rejection.
    setAdding(false);
    setReport({ text: "The add was cancelled. Nothing was sent." });
  };

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

  /** Send the ciphertext the dialog produced. The passphrase itself never reaches this component. */
  const answerPrompt = async (encryptedAnswer: Uint8Array) => {
    if (outstandingPrompt === null || !client) return;
    // One prompt accepts exactly one answer, so the question is closed here rather than when the
    // send comes back.
    setHandledPromptId(outstandingPrompt.promptId);
    try {
      const response = await client.answerHostPrompt({
        sessionToken: sessionToken ?? "",
        daemonInstanceId: instanceId,
        promptId: outstandingPrompt.promptId,
        encryptedAnswer,
      });
      if (!response.accepted) {
        // The dialog is gone by now, so this is the only place the refusal can be read. The
        // daemon's own words stand in: it distinguishes an unknown prompt from an expired one from
        // one already answered, and each sends the operator somewhere different.
        setReport({
          text:
            response.rejectionReason === ""
              ? "The host refused that answer."
              : `The host refused that answer: ${response.rejectionReason}`,
        });
      }
    } catch (error) {
      setReport({ text: `The answer could not be sent: ${messageOf(error)}` });
    }
  };

  return (
    <span
      data-testid={`hosts-row-${instanceId}-add-key`}
      className="flex items-baseline gap-1"
    >
      {candidates.length > 0 && (
        <select
          data-testid={`hosts-row-${instanceId}-add-key-choices`}
          aria-label={`Keys ${instanceId} offered to load into its ssh-agent`}
          // The picked path *is* the subject: it goes back to the host unchanged, because the
          // confinement it is read under is lexical and would refuse anything this side adorned.
          value={pickedPath}
          disabled={adding}
          onChange={(event) => setSubject(event.target.value)}
          className="border border-border rounded px-1 text-xs bg-background font-mono max-w-64"
        >
          <option value="">Pick a key…</option>
          {candidates.map((candidate) => (
            <option key={candidate.path} value={candidate.path}>
              {`${candidate.path} — ${candidate.keyType} ${candidate.fingerprint}`}
            </option>
          ))}
        </select>
      )}
      <input
        data-testid={`hosts-row-${instanceId}-add-key-subject`}
        type="text"
        aria-label={`Private key to load into ${instanceId}'s ssh-agent`}
        value={subject}
        disabled={adding}
        onChange={(event) => setSubject(event.target.value)}
        // An absolute path, because that is the only kind the host accepts and the only kind the
        // picker beside this field offers. `~` is expanded nowhere.
        placeholder="/home/you/.ssh/id_ed25519"
        className="border border-border rounded px-1 text-xs bg-background font-mono w-48"
      />
      <button
        type="button"
        data-testid={`hosts-row-${instanceId}-add-key-start`}
        onClick={() => void startAdd()}
        disabled={adding || subject.trim() === ""}
        className="px-2 border border-border rounded hover:bg-muted disabled:opacity-50"
      >
        Add
      </button>
      {report !== null && (
        <span data-testid={`hosts-row-${instanceId}-add-key-outcome`} title={report.title}>
          {report.text}
        </span>
      )}
      {addedFingerprint !== null && (
        <span
          data-testid={`hosts-row-${instanceId}-add-key-added`}
          className="break-all font-mono"
        >
          {addedFingerprint}
        </span>
      )}
      {outstandingPrompt !== null && (
        <HostPassphraseDialog
          hostId={instanceId}
          subject={outstandingPrompt.subject}
          // The key in hand, and only what has been established about *it*. Both derived facts come
          // from `answering`, whose prompt is this one, so the fingerprint on screen is the digest
          // of the very bytes beside it — never the string advertised with them, and never the
          // previous key's. Until that pair exists there is nothing to say and nothing to send.
          fingerprint={answering?.check.fingerprint ?? null}
          spkiDer={outstandingPrompt.hostPublicKey}
          keyContinuity={answering?.check.verdict ?? { kind: "unchecked" }}
          onSubmit={(encryptedAnswer) => void answerPrompt(encryptedAnswer)}
          onAcceptChangedKey={acceptChangedKey}
          onCancel={cancelAdd}
        />
      )}
    </span>
  );
}

/**
 * What an operator is told when the path they typed is not one the host can accept.
 *
 * Says what to do about it, which the daemon's own refusal deliberately cannot: `KEY_OUTSIDE_HOME`
 * names no path, so a `~` sent to the host comes back as "a key must be a path inside your own home"
 * about a path that was inside it.
 */
const ABSOLUTE_PATH_REQUIRED =
  "Name the key by its absolute path on this host, starting with “/” — “~” is not expanded anywhere.";

/**
 * Whether `subject` is a path the host's confinement can accept.
 *
 * The same lexical question `confined_to_home` asks, restated on the side that can explain the
 * answer. Nothing here resolves anything: a path is absolute or it is not.
 */
function isAnAbsolutePath(subject: string): boolean {
  return subject.trim().startsWith("/");
}

/** The one line of an error worth putting in front of an operator. */
function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
