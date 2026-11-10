/**
 * Merge the registered host-directory sources into the one list every host-selection surface reads.
 *
 * The merge rules are pure and live here so they are testable without a rendered provider — the
 * same reason `routing/selectedHost.ts` holds the selection rules. `SelectedDaemonProvider` calls
 * `useHostDirectory` and owns nothing about how the list is assembled.
 */

import { createContext, useContext, useMemo, type ReactNode } from "react";
import type { ConnectionStatus } from "../connections/types";
import type { HostDescriptor, HostDirectory, HostDirectorySource } from "./types";

/**
 * Merge `sources` into a directory.
 *
 * De-duplication is by `hostId`, first source wins. Order is therefore precedence, and it is
 * load-bearing once: the desktop app puts its own source first, so its description of its own host
 * beats the common room's advertisement of the same machine.
 *
 * The overall status is deliberately optimistic — `connected` as soon as any source is connected —
 * because one working source is a usable directory. A desktop app whose LiveKit peers are
 * unreachable can still use its own host, and a pessimistic rule would show it a connection error
 * for a feature it never asked for.
 */
export function mergeHostDirectory(sources: readonly HostDirectorySource[]): HostDirectory {
  const status = directoryStatusOf(sources);
  return {
    hosts: hostsOf(sources),
    sources,
    status,
    // Only surfaced when the directory as a whole is unusable. A failure on one source while
    // another still names hosts belongs to that source and is read off `sources` — publishing it
    // here would put a LiveKit error on a screen that is talking to its local host perfectly well.
    error: status === "error" ? firstErrorOf(sources) : null,
  };
}

/**
 * The overall status of a set of sources.
 *
 * `connected` if any source is; otherwise `connecting` if any is; otherwise `error` if any is;
 * otherwise `idle`. An unconfigured source reports `idle` and so never drags the directory into
 * `error` — that is what makes an absent LiveKit configuration a choice rather than a fault.
 */
export function directoryStatusOf(sources: readonly HostDirectorySource[]): ConnectionStatus {
  if (sources.some((source) => source.status === "connected")) return "connected";
  if (sources.some((source) => source.status === "connecting")) return "connecting";
  if (sources.some((source) => source.status === "error")) return "error";
  return "idle";
}

/** The hosts of `sources`, de-duplicated by `hostId`, first source winning. */
export function hostsOf(sources: readonly HostDirectorySource[]): readonly HostDescriptor[] {
  const hosts: HostDescriptor[] = [];
  const seen = new Set<string>();
  for (const source of sources) {
    for (const host of source.hosts) {
      // Order within a source is the source's own — the LiveKit one already orders by the room's
      // participant ordering, and re-sorting here would make the selector jump under the operator.
      if (seen.has(host.hostId)) continue;
      seen.add(host.hostId);
      hosts.push(host);
    }
  }
  return hosts;
}

/** The first source with something to say about why it is unusable. */
function firstErrorOf(sources: readonly HostDirectorySource[]): string | null {
  return sources.find((source) => source.error !== null)?.error ?? null;
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

const HostDirectoryContext = createContext<readonly HostDirectorySource[] | null>(null);

/**
 * What a component outside the provider reads. Module-level and never mutated, so the merge below
 * memoises on it and a directory-less subtree does not re-derive an empty directory every render.
 */
const NO_SOURCES: readonly HostDirectorySource[] = Object.freeze([]);

export interface HostDirectorySourcesProps {
  /**
   * The sources, in precedence order. The app root supplies the LiveKit and serving-host sources;
   * the desktop build prepends its own; a test supplies whatever it is about.
   */
  sources: readonly HostDirectorySource[];
  children: ReactNode;
}

/** Provide the directory's sources to the component subtree. Mount once near the app root. */
export function HostDirectorySources({ sources, children }: HostDirectorySourcesProps) {
  return (
    <HostDirectoryContext.Provider value={sources}>{children}</HostDirectoryContext.Provider>
  );
}

/**
 * The merged directory in scope.
 *
 * With no provider — a component rendered outside the tree — the directory is empty and `idle`,
 * which is the same shape as "nothing has been asked of this yet" and never an error.
 */
export function useHostDirectory(): HostDirectory {
  const sources = useContext(HostDirectoryContext) ?? NO_SOURCES;
  return useMemo(() => mergeHostDirectory(sources), [sources]);
}

/**
 * One named source's own status and hosts, or `undefined` when nothing registered under that id.
 *
 * For a surface that is *about* one source rather than about the fleet — the LiveKit presence
 * screen is the only one today, and it has to say "the common room could not be joined" even while
 * the directory as a whole is perfectly healthy on another source.
 */
export function useHostDirectorySource(sourceId: string): HostDirectorySource | undefined {
  const { sources } = useHostDirectory();
  return useMemo(() => sources.find((source) => source.id === sourceId), [sources, sourceId]);
}
