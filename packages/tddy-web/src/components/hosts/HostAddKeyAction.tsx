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
 * The prompt feed is read only while an add is in flight. The daemon replays whatever is still
 * outstanding to a subscriber that arrives late (`host_prompt_stream.rs`), so there is no window in
 * which this component can miss the question its own call raised — and a screenful of hosts nobody
 * is adding a key to opens no streams at all.
 *
 * PRD: `docs/ft/web/1-WIP/PRD-2026-09-06-agent-add-key.md`
 */

import React, { useEffect, useRef, useState } from "react";
import {
  AddHostKeyOutcome,
  ConnectionService,
  ProbeOutcome,
  type AddHostKeyResponse,
  type HostSshAgent,
} from "../../gen/connection_pb";
import { useHostClient } from "../../rpc/connections/registry";
import { useHostPrompts } from "../../rpc/useHostPrompts";
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
  const client = useHostClient(ConnectionService, instanceId);
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

  const [keyCheck, setKeyCheck] = useState<HostKeyCheck | null>(null);

  // Checked once per question, after the commit rather than during render: the check *records* the
  // pin, and a render React discards would otherwise spend this host's one first-use trust slot on
  // a dialog nobody was ever shown.
  //
  // The key itself is checked, not the fingerprint string beside it. Both fields ride the same
  // unauthenticated channel, and only the key encrypts anything — so `verifyHostKey` derives a
  // fingerprint from the published key, pins that, and refuses a prompt whose two halves disagree.
  useEffect(() => {
    if (outstandingPrompt === null) {
      setKeyCheck(null);
      return;
    }
    // Deriving a digest is asynchronous, so a prompt replaced while one is in flight would
    // otherwise land its verdict on the question that succeeded it.
    let current = true;
    void verifyHostKey(
      instanceId,
      outstandingPrompt.hostPublicKey,
      outstandingPrompt.hostPublicKeyFingerprint,
    ).then((check) => {
      if (current) setKeyCheck(check);
    });
    return () => {
      current = false;
    };
  }, [instanceId, outstandingPrompt]);

  if (!anAgentAnswered(sshAgent)) {
    return null;
  }

  const startAdd = async () => {
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
    if (keyCheck === null || keyCheck.fingerprint === null) return;
    acceptChangedHostKey(instanceId, keyCheck.fingerprint);
    setKeyCheck({ ...keyCheck, verdict: { kind: "unchanged" } });
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
      <input
        data-testid={`hosts-row-${instanceId}-add-key-subject`}
        type="text"
        aria-label={`Private key to load into ${instanceId}'s ssh-agent`}
        value={subject}
        disabled={adding}
        onChange={(event) => setSubject(event.target.value)}
        placeholder="~/.ssh/id_ed25519"
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
      {outstandingPrompt !== null && keyCheck !== null && (
        <HostPassphraseDialog
          hostId={instanceId}
          subject={outstandingPrompt.subject}
          // The fingerprint of the key that arrived, never the string advertised beside it: what the
          // operator verifies out of band has to be the key their passphrase is encrypted under.
          fingerprint={keyCheck.fingerprint}
          spkiDer={outstandingPrompt.hostPublicKey}
          keyContinuity={keyCheck.verdict}
          onSubmit={(encryptedAnswer) => void answerPrompt(encryptedAnswer)}
          onAcceptChangedKey={acceptChangedKey}
          onCancel={cancelAdd}
        />
      )}
    </span>
  );
}

/** The one line of an error worth putting in front of an operator. */
function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
