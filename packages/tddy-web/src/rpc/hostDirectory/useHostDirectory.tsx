/**
 * Merge the registered host-directory sources into the one list every host-selection surface reads.
 *
 * The merge rules are pure and live here so they are testable without a rendered provider — the
 * same reason `routing/selectedHost.ts` holds the selection rules. `SelectedDaemonProvider` calls
 * `useHostDirectory` and owns nothing about how the list is assembled.
 */

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
  // TODO(host-directory): implement
  throw new Error(`mergeHostDirectory(${sources.length} sources) is not implemented yet`);
}

/**
 * The overall status of a set of sources.
 *
 * `connected` if any source is; otherwise `connecting` if any is; otherwise `error` if any is;
 * otherwise `idle`. An unconfigured source reports `idle` and so never drags the directory into
 * `error` — that is what makes an absent LiveKit configuration a choice rather than a fault.
 */
export function directoryStatusOf(sources: readonly HostDirectorySource[]): ConnectionStatus {
  // TODO(host-directory): implement
  throw new Error(`directoryStatusOf(${sources.length} sources) is not implemented yet`);
}

/** The hosts of `sources`, de-duplicated by `hostId`, first source winning. */
export function hostsOf(sources: readonly HostDirectorySource[]): readonly HostDescriptor[] {
  // TODO(host-directory): implement
  throw new Error(`hostsOf(${sources.length} sources) is not implemented yet`);
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

import { createContext, type ReactNode } from "react";

const HostDirectoryContext = createContext<readonly HostDirectorySource[] | null>(null);

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
  // TODO(host-directory): implement
  throw new Error("useHostDirectory is not implemented yet");
}
