/**
 * What a host has installed and configured: the git identity its commits would carry, and the state
 * of the GitHub CLI there.
 *
 * Six states must stay visually distinct, because each sends an operator somewhere different:
 *
 * | git                       | gh                          |
 * |---------------------------|-----------------------------|
 * | configured (name + email) | authenticated as `<login>`  |
 * | none configured           | installed but logged out    |
 * | probe failed              | not installed / probe failed |
 *
 * The `gh` login is **this host's**, not the tddy session's user in `UserAvatar` and not a
 * `GITHUB_TOKEN` in some environment. Three identities that can disagree, so the row labels which
 * one it is showing.
 */

import type {
  HostGitIdentity,
  HostGithubCli,
  HostRemoteDesktop,
  HostSshAgent,
} from "../../gen/connection_pb";
import { ProbeOutcome } from "../../gen/connection_pb";
import { HostRowRemoteDesktop } from "./HostRowRemoteDesktop";
import { HostRowSshAgent } from "./HostRowSshAgent";

export interface HostRowToolingProps {
  instanceId: string;
  git: HostGitIdentity | undefined;
  githubCli: HostGithubCli | undefined;
  /** Added by `#hosts-screen 5/8`, and optional for the same reason the two above accept
   * `undefined`: a row renders before any probe has answered for that host. */
  sshAgent?: HostSshAgent | undefined;
  /** Added by `#hosts-screen 7/8`: one reading per probed protocol. Optional for the same reason —
   * a row renders before any probe has answered. */
  remoteDesktop?: readonly HostRemoteDesktop[] | undefined;
}

/** What one cell says, and what hovering it explains. */
interface ToolingCell {
  text: string;
  title: string;
}

/** Nothing has answered for this host yet — distinct from every answer a probe can give. */
function awaitingProbe(subject: string): ToolingCell {
  return { text: "…", title: `Waiting for a ${subject} result from this host` };
}

/**
 * The outcomes that carry no finding, rendered before either block's own states are consulted.
 *
 * A probe that could not run is never dressed up as a negative: "could not check" and "not
 * configured" send an operator to two different places, and only one of them is a host to go and
 * fix. A missing block is the same admission — nothing has answered for this host yet — so it says
 * so rather than borrowing the shape of an answer.
 *
 * Only `OK` passes through to a finding, and the guard is written that way round on purpose.
 * proto3 enums are open and this message grows — nodes 5 and 7 of the stack extend it — so a newer
 * daemon can send a `ProbeOutcome` this bundle's generated enum has never heard of. Listing the
 * outcomes that mean "no finding" would let that unknown value fall through and render "Not
 * configured" / "Not installed" with total confidence about a probe whose result was not
 * understood. Listing the one outcome that licenses a finding cannot.
 */
function unanswered(
  outcome: ProbeOutcome,
  failureReason: string,
  subject: string,
): ToolingCell | null {
  switch (outcome) {
    case ProbeOutcome.OK:
      return null;
    // A block whose sender set no outcome states nothing, the same as no block at all.
    case ProbeOutcome.UNSPECIFIED:
      return awaitingProbe(subject);
    case ProbeOutcome.UNSUPPORTED:
      return { text: "Not supported here", title: `This host cannot run the ${subject}` };
    // FAILED, and every outcome a newer daemon may add that this bundle cannot name.
    default:
      return {
        text: "Could not check",
        title: failureReason
          ? `The ${subject} failed: ${failureReason}`
          : `The ${subject} failed, so nothing is known about this host`,
      };
  }
}

function gitCell(git: HostGitIdentity | undefined): ToolingCell {
  const subject = "git identity probe";
  if (git === undefined) {
    return awaitingProbe(subject);
  }
  const noFinding = unanswered(git.outcome, git.failureReason, subject);
  if (noFinding !== null) {
    return noFinding;
  }
  if (!git.configured) {
    return {
      text: "Not configured",
      title: "This host has no git user.name / user.email set for its OS user",
    };
  }
  return {
    text: `${git.userName} <${git.userEmail}>`,
    title: "The identity commits made on this host would carry",
  };
}

function githubCliCell(githubCli: HostGithubCli | undefined): ToolingCell {
  const subject = "gh probe";
  if (githubCli === undefined) {
    return awaitingProbe(subject);
  }
  const noFinding = unanswered(githubCli.outcome, githubCli.failureReason, subject);
  if (noFinding !== null) {
    return noFinding;
  }
  if (!githubCli.installed) {
    return { text: "Not installed", title: "The GitHub CLI is not on this host's PATH" };
  }
  if (!githubCli.authenticated) {
    return {
      text: "Not authenticated",
      title: "The GitHub CLI is installed on this host but logged out",
    };
  }
  return {
    text: githubCli.login,
    title: `This host's GitHub CLI is authenticated as ${githubCli.login}`,
  };
}

export function HostRowTooling({
  instanceId,
  git,
  githubCli,
  sshAgent,
  remoteDesktop,
}: HostRowToolingProps) {
  const gitState = gitCell(git);
  const ghState = githubCliCell(githubCli);

  return (
    <span
      data-testid={`hosts-row-${instanceId}-tooling`}
      className="flex items-center gap-3 text-xs text-muted-foreground"
    >
      {/* Both cells are labelled with the tool they speak for. Unlabelled, an authenticated `gh`
          renders as a bare login beside the row's other identities — the tddy session user in
          `UserAvatar`, the git identity next to it — and reads as whichever one the operator
          expected to see there. */}
      <span data-testid={`hosts-row-${instanceId}-git`} title={gitState.title}>
        <span className="mr-1 opacity-70">git</span>
        {gitState.text}
      </span>
      <span data-testid={`hosts-row-${instanceId}-gh`} title={ghState.title}>
        <span className="mr-1 opacity-70">gh</span>
        {ghState.text}
      </span>
      <HostRowSshAgent instanceId={instanceId} sshAgent={sshAgent} />
      {/* No readings is no claim: a host nothing has answered for yet renders an empty section
          rather than a protocol row asserting something about a probe that has not run. */}
      <HostRowRemoteDesktop instanceId={instanceId} readings={remoteDesktop ?? []} />
    </span>
  );
}
