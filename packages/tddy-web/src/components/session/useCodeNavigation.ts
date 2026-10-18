import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import type { CodeLocationRef, CodePosition } from "./codeNavigationApi";
import { NOTHING_DEFINABLE, ProbeStore, type DefinableSource } from "./codeProbes";
import { errorMessage, type NavigationContext } from "./navigationLookup";
import { useDefinitionLookup } from "./useDefinitionLookup";
import { useHoverQuery } from "./useHoverQuery";
import { useReferencesLookup } from "./useReferencesLookup";
import type {
  CodeQueryHandle,
  CodeQueryOutcome,
  CodeQueryResult,
  CodeSession,
} from "./useCodeSession";
import type { WorktreeFilesApi } from "./worktreeFilesApi";

export type SelectedFile = {
  relPath: string;
  content: string;
  error: boolean;
  /** One-based line the preview scrolls to and marks as the navigation target. */
  focusLine?: number;
};

/**
 * The hover card of the identifier at `at` in `relPath`: `loading` while the host holds the query
 * for a loading index, then the markdown it answered. The card finds its identifier by `at` each
 * time it lays out, so a re-rendered preview never leaves it anchored to a detached element.
 */
export type Hover = {
  at: CodePosition;
  relPath: string;
  content: { kind: "loading" } | { kind: "markdown"; markdown: string };
};

/** A list of locations the user picks from — the references of a symbol, or ambiguous definitions. */
export type LocationList = { title: string; locations: CodeLocationRef[] };

/** The navigation notice: a held query waiting for the index, or an outcome to read. */
export type Notice = { kind: "held" } | { kind: "message"; text: string; isError: boolean };

/** The transient navigation UI: hover card, location list and notice. */
function useNavigationUi() {
  const [hover, setHover] = useState<Hover | null>(null);
  const [locations, setLocations] = useState<LocationList | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);
  return { hover, setHover, locations, setLocations, notice, setNotice };
}

export type NavigationUi = ReturnType<typeof useNavigationUi>;

/**
 * At most one query of a kind in flight: a newer one, or `cancel`, drops the older on the host, and
 * only the latest one's outcome is delivered.
 */
function useLatestQuery(session: CodeSession | null) {
  const current = useRef<CodeQueryHandle | null>(null);

  const cancel = useCallback(() => {
    if (current.current !== null) session?.cancel(current.current.id);
    current.current = null;
  }, [session]);

  /** Run `handle`, delivering its outcome only while it is still the latest. */
  const track = useCallback(
    (
      handle: CodeQueryHandle,
      onOutcome: (result: CodeQueryResult) => void,
      onError: (e: unknown) => void,
    ) => {
      cancel();
      current.current = handle;
      const isLatest = () => current.current?.id === handle.id;
      handle.outcome.then(
        (outcome: CodeQueryOutcome) => {
          if (!isLatest()) return;
          current.current = null;
          if (outcome.kind === "answered") onOutcome(outcome.result);
        },
        (e: unknown) => {
          if (!isLatest()) return;
          current.current = null;
          onError(e);
        },
      );
    },
    [cancel],
  );

  const isLatest = useCallback((id: bigint) => current.current?.id === id, []);

  return useMemo(() => ({ track, cancel, isLatest }), [track, cancel, isLatest]);
}

export type LatestQuery = ReturnType<typeof useLatestQuery>;

/** The pane's two query slots: the latest hover, and the latest definition or references lookup. */
function useQueries(session: CodeSession | null): NavigationContext["queries"] {
  const hover = useLatestQuery(session);
  const lookup = useLatestQuery(session);
  return useMemo(() => ({ hover, lookup }), [hover, lookup]);
}

/** The file shown in the preview; opening one dismisses the navigation UI of the previous one. */
function useSelectedFile(api: WorktreeFilesApi, onOpen: () => void) {
  const [selected, setSelected] = useState<SelectedFile | null>(null);

  const openFile = useCallback(
    (relPath: string, focusLine?: number) => {
      onOpen();
      void api
        .readFile(relPath)
        .then((res) =>
          setSelected({
            relPath,
            content: res.contentUtf8,
            error: false,
            focusLine,
          }),
        )
        .catch((e: unknown) => {
          setSelected({
            relPath,
            content: errorMessage(e, "Failed to read file"),
            error: true,
          });
        });
    },
    [api, onOpen],
  );

  return { selected, openFile };
}

/** The pane's probes over `session`; those in flight are cancelled when the session goes. */
function useProbes(session: CodeSession | null): ProbeStore | null {
  const probes = useMemo(() => (session === null ? null : new ProbeStore(session)), [session]);
  useEffect(() => () => probes?.forget(), [probes]);
  return probes;
}

/** Dismiss the navigation UI and drop the queries behind it. */
function useClearNavigation(
  { setHover, setLocations, setNotice }: NavigationUi,
  queries: NavigationContext["queries"],
  probes: ProbeStore | null,
) {
  return useCallback(() => {
    queries.hover.cancel();
    queries.lookup.cancel();
    // Positions are remembered against the content the preview showed; a newly read file may differ.
    probes?.forget();
    setHover(null);
    setLocations(null);
    setNotice(null);
  }, [queries, probes, setHover, setLocations, setNotice]);
}

/**
 * The pointer dwelling on identifiers of the selected file: `dwell` probes the one at `at` for an
 * underline and asks its hover; `definable` says which ones a probe found a definition for.
 */
function useProbeDwell(
  probes: ProbeStore | null,
  selectedPath: string | null,
  showHover: (at: CodePosition) => void,
) {
  const dwell = useCallback(
    (at: CodePosition) => {
      if (selectedPath === null) return;
      probes?.probe(selectedPath, at);
      showHover(at);
    },
    [selectedPath, probes, showHover],
  );

  const definable: DefinableSource = useMemo(
    () =>
      probes === null || selectedPath === null ? NOTHING_DEFINABLE : probes.sourceFor(selectedPath),
    [probes, selectedPath],
  );

  return { dwell, definable };
}

/**
 * The preview's selected file together with the navigation state and handlers (definition, hover,
 * references, opening a location) that act on it, asked over the pane's `CodeSession`.
 *
 * A query the loading index holds shows as held — the hover card loading, the notice saying so —
 * until its answer arrives; a newer hover, closing the card or opening another file drops it.
 */
export function useCodeNavigation(api: WorktreeFilesApi, session: CodeSession | null) {
  const ui = useNavigationUi();
  const queries = useQueries(session);
  const probes = useProbes(session);
  const clear = useClearNavigation(ui, queries, probes);
  const { selected, openFile } = useSelectedFile(api, clear);
  const selectedPath = selected && !selected.error ? selected.relPath : null;

  const context: NavigationContext = { session, selectedPath, openFile, ui, probes, queries };
  const { openLocation, navigate } = useDefinitionLookup(context);
  const showHover = useHoverQuery(context);
  const showReferences = useReferencesLookup(context);
  const { dwell, definable } = useProbeDwell(probes, selectedPath, showHover);

  const { setHover } = ui;
  const closeHover = useCallback(() => {
    queries.hover.cancel();
    setHover(null);
  }, [queries.hover, setHover]);

  return {
    selected,
    openFile,
    hover: ui.hover,
    locations: ui.locations,
    notice: ui.notice,
    closeHover,
    dwell,
    definable,
    openLocation,
    navigate,
    showReferences,
  };
}
