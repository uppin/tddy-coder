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
import type { DerivedHost, HostService } from "../gen/host_pb";
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
  // TODO(derived-hosts): implement
  void host;
  return { hostId: "", label: "", sourceId: DERIVED_SOURCE_ID };
}

/**
 * Ask every connected parent for the hosts it reaches, and merge the answers into one source.
 *
 * Hosts keep the parents' order, and each parent's own listing order within it. The source is
 * `connected` once any parent has listed, `idle` when no parent is connected, and `error` only
 * when every parent that was asked failed.
 */
export async function listDerivedHosts(
  parents: readonly DerivedHostParent[],
  sessionToken: string,
): Promise<DerivedHostDirectorySource> {
  // TODO(derived-hosts): implement
  void parents;
  void sessionToken;
  return NO_DERIVED_HOSTS;
}
