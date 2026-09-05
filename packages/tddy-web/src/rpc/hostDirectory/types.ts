/**
 * The host directory: who this page can talk to, and how sure it is.
 *
 * Knowing which hosts exist was, until now, the same thing as being joined to a LiveKit common
 * room: `SelectedDaemonProvider` connected a `Room` and read the host list off its participants
 * (`daemonHostsFromParticipants`). With no LiveKit configuration the list was empty — including of
 * the daemon serving the page, which `/api/config` has always named.
 *
 * A directory is the merge of several {@link HostDirectorySource}s. A browser page has the LiveKit
 * source and the serving-host source; the desktop app adds one of its own. Nothing here knows what
 * a room is.
 *
 * Technical: `packages/tddy-web/docs/host-directory.md`.
 */

import type { ConnectionStatus } from "../connections/types";

/**
 * A host this page could select, as some source described it.
 *
 * The optional fields are advertised by newer daemons only; a source that does not know them leaves
 * them out, and every consumer already treats absence as "unadvertised" rather than as zero.
 */
export interface HostDescriptor {
  /** The daemon instance id. Also the key a `HostConnection` is resolved by. */
  readonly hostId: string;

  readonly label: string;

  /** Which source contributed this entry. Diagnostics, and de-duplication precedence. */
  readonly sourceId: string;

  /** The host's base clone location, relative to each OS user's home (`repos_base_path`). */
  readonly reposBasePath?: string;

  /** The largest single session attachment this host will serve (`max_attachment_bytes`). */
  readonly maxAttachmentBytes?: number;
}

/**
 * One contributor to the directory.
 *
 * A source that is not configured contributes nothing and reports `idle` — **not** `error`. That
 * distinction is the whole of "LiveKit is optional": an unconfigured common room is a choice, and a
 * directory that called it a fault would make every desktop screen show a connection error for a
 * feature nobody asked for.
 */
export interface HostDirectorySource {
  /** Stable identifier (`"livekit"`, `"serving"`, later `"local-ipc"`). */
  readonly id: string;

  readonly status: ConnectionStatus;

  /** Why this source is unusable, when {@link status} is `"error"`; `null` otherwise. */
  readonly error: string | null;

  readonly hosts: readonly HostDescriptor[];
}

/** The merged view every host-selection surface reads. */
export interface HostDirectory {
  /**
   * Every host any source knows, de-duplicated by `hostId`. The first source to contribute a host
   * wins, which is the same precedence rule the connection registry uses, for the same reason: the
   * desktop's own description of its host should beat a common-room advertisement of it.
   */
  readonly hosts: readonly HostDescriptor[];

  /** Each source's own status, so one dead source can be reported without condemning the rest. */
  readonly sources: readonly HostDirectorySource[];

  /**
   * The directory's overall status, for the selector chrome that used to read `roomStatus`.
   *
   * `connected` as soon as **any** source is connected, because one working source is a usable
   * directory; `error` only when every source that was asked for is in error. A desktop app whose
   * LiveKit peers are unreachable is still perfectly able to use its own host, and must not be told
   * otherwise.
   */
  readonly status: ConnectionStatus;

  /** The first error among the sources, when {@link status} is `"error"`; `null` otherwise. */
  readonly error: string | null;
}
