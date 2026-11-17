/**
 * The hosts each connected daemon can reach through a remote shell, as a host-directory source.
 *
 * A *derived* host is a machine with no daemon of its own — an alias in a daemon's `~/.ssh/config`
 * is the first kind — that the daemon can run a session's agent on. The daemon that reaches it is
 * its **parent**. Each connected daemon is asked for its own (`HostService.ListDerivedHosts`), and
 * every host it answers becomes a {@link HostDescriptor} carrying `parent` and `target`.
 *
 * A derived host is a view over a target, not a connection: it has no wire of its own in the
 * browser, so this source needs no capability and no `ConnectionProvider`. Selecting one starts a
 * session on the parent with the descriptor's `target`.
 *
 * Three rules, each pinned in `derivedSource.test.ts`:
 *
 * - **Only a connected parent is asked.** A derived host is reachable through its parent and
 *   nothing else, so an offline parent's hosts are not offered — and are not remembered from the
 *   last time it answered.
 * - **A failure belongs to its parent.** A daemon whose SSH config cannot be read answers `FAILED`;
 *   that is recorded against that daemon in {@link DerivedHostDirectorySource.failures} and never
 *   withholds another daemon's hosts.
 * - **A failed listing is not an empty one.** `OK` with no hosts is a daemon that reaches none.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-15-remote-shell-derived-hosts.md.
 */

import type { Client } from "@connectrpc/connect";
import type { ConnectionStatus } from "../connections/types";
import { ProbeOutcome, type DerivedHost, type HostService } from "../gen/host_pb";
import type { HostDescriptor, HostDirectorySource } from "./types";

/** The id this source contributes under. Precedence is stated against it, so it is a constant. */
export const DERIVED_SOURCE_ID = "derived";

/** One daemon the directory lists, as a possible parent of derived hosts. */
export interface DerivedHostParent {
  /** The daemon's own directory entry. Its `hostId` is what its derived hosts name as `parent`. */
  readonly host: HostDescriptor;

  /** How the connection to this daemon is doing. Only a `"connected"` parent is asked. */
  readonly status: ConnectionStatus;

  /** `HostService` on this daemon, over that connection. */
  readonly client: Client<typeof HostService>;
}

/** A parent that could not list the hosts it reaches, and why. */
export interface DerivedHostListingFailure {
  /** The `hostId` of the daemon whose listing failed. */
  readonly parentHostId: string;

  /** The daemon's own reason (`failure_reason`), or the call's error when it never answered. */
  readonly reason: string;
}

/** The derived source: the directory's contract, plus each parent's failure. */
export interface DerivedHostDirectorySource extends HostDirectorySource {
  /**
   * Every connected parent whose listing failed, in the order the parents were given. Kept apart
   * from `error`, which is for a source that is unusable as a whole: one parent's unreadable SSH
   * config leaves every other parent's hosts in `hosts`.
   */
  readonly failures: readonly DerivedHostListingFailure[];
}

/** What the source is before any parent has been asked: nothing, and not a fault. */
export const NO_DERIVED_HOSTS: DerivedHostDirectorySource = Object.freeze({
  id: DERIVED_SOURCE_ID,
  status: "idle",
  error: null,
  hosts: Object.freeze([]),
  failures: Object.freeze([]),
});

/**
 * Describe one listed derived host as this source's contribution to the directory: its id and
 * label as listed, `parent` its `parentInstanceId`, `target` its target.
 */
export function derivedHostDescriptorOf(host: DerivedHost): HostDescriptor {
  return {
    hostId: host.id,
    label: host.label,
    sourceId: DERIVED_SOURCE_ID,
    parent: host.parentInstanceId,
    ...(host.target !== undefined ? { target: host.target } : {}),
    // Usability is read from the reason: a host whose daemon gives none — an older daemon, or
    // a fixture without the fields — stays usable, so only a named reason withholds a host.
    ...(host.unusableReason
      ? { usable: false, unusableReason: host.unusableReason }
      : {}),
  };
}

// ---------------------------------------------------------------------------
// One parent at a time (`#remote-shell` 9/11)
// ---------------------------------------------------------------------------

/**
 * How long one parent is given to list its hosts before the call counts as that parent's failure.
 * A unary call to a listed peer over the room may never answer; without a bound, one such daemon
 * would hold every daemon's hosts back for the life of the page.
 */
export const DERIVED_LISTING_DEADLINE_MS = 20_000;

/** One parent's answer: what it listed, and its failure when it could not list or did not answer. */
export interface ParentDerivedHosts {
  readonly parentHostId: string;
  readonly hosts: readonly HostDescriptor[];
  readonly failure: DerivedHostListingFailure | null;
}

/**
 * Ask one connected parent for the hosts it reaches, within `deadlineMs`.
 *
 * Never rejects: a refused call, a `FAILED` outcome and a call past its deadline are each that
 * parent's `failure`, with its hosts (a `FAILED` listing still carries the hosts that arrived).
 */
export async function askParentForDerivedHosts(
  parent: DerivedHostParent,
  sessionToken: string,
  deadlineMs: number = DERIVED_LISTING_DEADLINE_MS,
): Promise<ParentDerivedHosts> {
  const parentHostId = parent.host.hostId;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const deadline = new Promise<ParentDerivedHosts>((resolve) => {
    timer = setTimeout(() => {
      resolve({
        parentHostId,
        hosts: [],
        failure: {
          parentHostId,
          reason: `${parentHostId} did not list its hosts within ${deadlineMs} ms`,
        },
      });
    }, deadlineMs);
  });
  const answer = (async (): Promise<ParentDerivedHosts> => {
    try {
      const response = await parent.client.listDerivedHosts({
        sessionToken,
        daemonInstanceId: parentHostId,
      });
      const hosts = response.hosts.map(derivedHostDescriptorOf);
      if (response.outcome === ProbeOutcome.FAILED) {
        return {
          parentHostId,
          hosts,
          failure: {
            parentHostId,
            reason: response.failureReason || "listing derived hosts failed",
          },
        };
      }
      return { parentHostId, hosts, failure: null };
    } catch (error) {
      return {
        parentHostId,
        hosts: [],
        failure: {
          parentHostId,
          reason: error instanceof Error ? error.message : String(error),
        },
      };
    }
  })();
  try {
    return await Promise.race([answer, deadline]);
  } finally {
    if (timer !== undefined) clearTimeout(timer);
  }
}

/** Each parent's latest answer, by source object — including an answer that listed no hosts. */
const answersBySource = new WeakMap<
  DerivedHostDirectorySource,
  ReadonlyMap<string, ParentDerivedHosts>
>();

function answersOf(source: DerivedHostDirectorySource): Map<string, ParentDerivedHosts> {
  const known = answersBySource.get(source);
  if (known) return new Map(known);
  const byParent = new Map<string, ParentDerivedHosts>();
  for (const host of source.hosts) {
    if (!host.parent) continue;
    const existing = byParent.get(host.parent);
    byParent.set(host.parent, {
      parentHostId: host.parent,
      hosts: [...(existing?.hosts ?? []), host],
      failure: existing?.failure ?? null,
    });
  }
  for (const failure of source.failures) {
    const existing = byParent.get(failure.parentHostId);
    byParent.set(failure.parentHostId, {
      parentHostId: failure.parentHostId,
      hosts: existing?.hosts ?? [],
      failure,
    });
  }
  return byParent;
}

/**
 * Fold one parent's answer into the source: that parent's earlier hosts and failure are replaced,
 * every other parent's are kept, and parents stay in the order they first answered. The source is
 * `connected` once any parent has answered, `error` only when every parent that answered failed.
 */
export function mergeParentAnswer(
  previous: DerivedHostDirectorySource,
  answer: ParentDerivedHosts,
): DerivedHostDirectorySource {
  const byParent = answersOf(previous);
  byParent.set(answer.parentHostId, answer);
  const answers = [...byParent.values()];
  const hosts = answers.flatMap((entry) => entry.hosts);
  const failures = answers.flatMap((entry) => (entry.failure ? [entry.failure] : []));
  const allFailed = answers.length > 0 && answers.every((entry) => entry.failure !== null);
  const merged: DerivedHostDirectorySource = allFailed
    ? {
        id: DERIVED_SOURCE_ID,
        status: "error",
        error: failures[0]?.reason ?? "listing derived hosts failed",
        hosts,
        failures,
      }
    : { id: DERIVED_SOURCE_ID, status: "connected", error: null, hosts, failures };
  answersBySource.set(merged, byParent);
  return merged;
}

/**
 * Which parents to ask now: every connected parent not yet asked, and — after a refresh — every
 * connected parent again. A parent that is not `connected` is never asked, whatever `asked` says.
 */
export function parentsToAsk(
  parents: readonly DerivedHostParent[],
  asked: ReadonlySet<string>,
  refreshed: boolean,
): DerivedHostParent[] {
  return parents.filter(
    (parent) =>
      parent.status === "connected" && (refreshed || !asked.has(parent.host.hostId)),
  );
}
