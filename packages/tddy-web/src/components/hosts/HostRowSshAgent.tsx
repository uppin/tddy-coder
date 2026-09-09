/**
 * The ssh-agent a host has, and the keys it is holding.
 *
 * Four states, kept distinct because each points somewhere different:
 *
 * | State | What an operator does about it |
 * |---|---|
 * | agent holding keys | nothing — this host can reach its remotes |
 * | agent, no keys | add a key (`#hosts-screen 6/8`) |
 * | no agent reachable | start an agent, or the daemon cannot see its socket |
 * | probe failed | look at the daemon log; we do not know |
 *
 * ⚠ A key's `comment` is free text set when the key was generated — commonly `user@host`. It is
 * **not** a file path, and the agent does not know which file a key came from, so the row must never
 * present it as a location.
 */

import type { HostSshAgent, SshAgentKey } from "../../gen/connection_pb";
import { ProbeOutcome } from "../../gen/connection_pb";

export interface HostRowSshAgentProps {
  instanceId: string;
  sshAgent: HostSshAgent | undefined;
}

/** What the section says when it is not listing keys, and what hovering it explains. */
interface AgentSummary {
  text: string;
  title: string;
}

/**
 * The summary this state deserves, or `null` when the state is a key list to render.
 *
 * A probe that could not run is never dressed up as a negative: "could not check" and "no agent"
 * send an operator to two different places, and only one of them is a host to go and start
 * something on. An agent holding nothing is a third: it is running, and wants a key.
 */
/** Nothing has answered for this host yet — distinct from every answer the probe can give. */
function awaitingAgentProbe(): AgentSummary {
  return { text: "…", title: "Waiting for an ssh-agent result from this host" };
}

function summaryOf(sshAgent: HostSshAgent | undefined): AgentSummary | null {
  // A block that is absent states nothing, the same as one whose sender set no outcome.
  if (sshAgent === undefined) {
    return awaitingAgentProbe();
  }
  // Only `OK` passes through to a finding, and the guard is written that way round on purpose —
  // the same reasoning `unanswered` in `HostRowTooling.tsx` spells out for the git and `gh` blocks.
  // proto3 enums are open and a newer daemon can send a `ProbeOutcome` this bundle's generated enum
  // has never heard of. Listing the outcomes that mean "no finding" would let that unknown value
  // fall through to `reachable`, and a probe whose result was not understood would render as
  // "No agent" — sending an operator to start an agent that is very possibly already running.
  // Listing the one outcome that licenses a finding cannot do that.
  switch (sshAgent.outcome) {
    case ProbeOutcome.OK:
      break;
    case ProbeOutcome.UNSPECIFIED:
      return awaitingAgentProbe();
    case ProbeOutcome.UNSUPPORTED:
      return { text: "Not supported here", title: "This host cannot run the ssh-agent probe" };
    // FAILED, and every outcome a newer daemon may add that this bundle cannot name.
    default:
      return {
        text: "Could not check",
        title: sshAgent.failureReason
          ? `The ssh-agent probe failed: ${sshAgent.failureReason}`
          : "The ssh-agent probe failed, so nothing is known about this host's keys",
      };
  }
  if (!sshAgent.reachable) {
    return {
      text: "No agent",
      title: "No ssh-agent answered for this host's OS user",
    };
  }
  if (sshAgent.keys.length === 0) {
    return {
      text: "No keys loaded",
      title: "An ssh-agent is running for this host's OS user and is holding no keys",
    };
  }
  return null;
}

/**
 * One held key: its type, its whole fingerprint, and its comment.
 *
 * The fingerprint is rendered in full. Two keys can share any prefix of one, so a shortened
 * fingerprint identifies nothing an operator could match against their own `ssh-add -l`.
 *
 * The comment is rendered as what it is — free text — with no label suggesting it locates anything.
 */
function HeldKey({ instanceId, sshKey }: { instanceId: string; sshKey: SshAgentKey }) {
  return (
    <span
      data-testid={`hosts-row-${instanceId}-ssh-key-${sshKey.fingerprint}`}
      className="flex items-baseline gap-1"
      title={`An ssh-agent on this host is holding this ${sshKey.keyType} key`}
    >
      <span className="opacity-70">{sshKey.keyType}</span>
      <span className="break-all font-mono">{sshKey.fingerprint}</span>
      {sshKey.comment ? <span className="italic">{sshKey.comment}</span> : null}
    </span>
  );
}

export function HostRowSshAgent({ instanceId, sshAgent }: HostRowSshAgentProps) {
  const summary = summaryOf(sshAgent);

  return (
    <span
      data-testid={`hosts-row-${instanceId}-ssh-agent`}
      className="flex items-baseline gap-1 text-xs text-muted-foreground"
    >
      {/* Labelled, like the git and gh cells beside it: an unlabelled fingerprint beside a git
          identity and a gh login reads as whichever of them the operator expected to see. */}
      <span className="opacity-70">ssh-agent</span>
      {summary === null ? (
        <span className="flex flex-col gap-1">
          {(sshAgent?.keys ?? []).map((sshKey) => (
            <HeldKey key={sshKey.fingerprint} instanceId={instanceId} sshKey={sshKey} />
          ))}
        </span>
      ) : (
        <span title={summary.title}>{summary.text}</span>
      )}
    </span>
  );
}
