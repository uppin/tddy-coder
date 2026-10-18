import { useCallback } from "react";

import { CodeIndexState, CodeQueryKind } from "../../gen/code_navigation_pb";
import { toLocationRef, type CodeLocationRef, type CodePosition } from "./codeNavigationApi";
import type { ProbeStore } from "./codeProbes";
import type { LatestQuery, NavigationUi, Notice } from "./useCodeNavigation";
import type { CodeQueryResult, CodeSession } from "./useCodeSession";

export const errorMessage = (e: unknown, fallback: string) =>
  e instanceof Error ? e.message : fallback;

const NOT_LOADED = "The code index has not loaded — nothing found yet";

/** A notice to read that is not an error. */
export const infoNotice = (text: string): Notice => ({ kind: "message", text, isError: false });

/** What the navigation hooks act on: the selected file, the session asked, and the UI they drive. */
export type NavigationContext = {
  session: CodeSession | null;
  selectedPath: string | null;
  openFile: (relPath: string, focusLine?: number) => void;
  ui: NavigationUi;
  probes: ProbeStore | null;
  queries: { hover: LatestQuery; lookup: LatestQuery };
};

/** Each location lookup: the query it asks and the label its failure is reported under. */
const LOOKUPS = {
  definition: { kind: CodeQueryKind.DEFINITION, label: "Go to definition" },
  references: { kind: CodeQueryKind.REFERENCES, label: "Find references" },
} as const;

type LookupKind = keyof typeof LOOKUPS;

/** `(what) => (error) => …`: report that `what` failed as an error notice. */
export function useShowError(setNotice: NavigationUi["setNotice"]) {
  return useCallback(
    (what: string) => (e: unknown) =>
      setNotice({
        kind: "message",
        text: `${what} failed: ${errorMessage(e, "unknown error")}`,
        isError: true,
      }),
    [setNotice],
  );
}

/** The locations a `which` answer carries, and whether the index had loaded when it answered. */
function locationsOf(result: CodeQueryResult, which: LookupKind) {
  if (result.case !== which) return null;
  if (result.case !== "definition" && result.case !== "references") return null;
  return {
    found: result.value.locations.map(toLocationRef),
    ready: result.value.state === CodeIndexState.READY,
  };
}

/**
 * Ask the `which` lookup of the identifier at `at` in `relPath` as the latest lookup: the notice
 * reads held while the index holds it, then `onFound` gets its locations — unless it found none
 * because the index has not loaded, which the notice says instead.
 */
export function useLookup({ session, ui: { setNotice }, queries: { lookup } }: NavigationContext) {
  const showError = useShowError(setNotice);
  return useCallback(
    (
      which: LookupKind,
      relPath: string,
      at: CodePosition,
      onFound: (found: CodeLocationRef[]) => void,
    ) => {
      if (session === null) return;
      const { kind, label } = LOOKUPS[which];
      const handle = session.ask(kind, relPath, at, {
        onHeld: () => {
          if (lookup.isLatest(handle.id)) setNotice({ kind: "held" });
        },
      });
      lookup.track(
        handle,
        (result) => {
          setNotice(null);
          if (result.case === "error") return showError(label)(new Error(result.value));
          const answer = locationsOf(result, which);
          if (answer === null) return;
          if (answer.found.length === 0 && !answer.ready) return setNotice(infoNotice(NOT_LOADED));
          onFound(answer.found);
        },
        showError(label),
      );
    },
    [session, lookup, setNotice, showError],
  );
}
