import { useCallback, useMemo, useRef, useState } from "react";

import { CodeIndexState, CodeQueryKind, Definability } from "../../gen/code_navigation_pb";
import { toLocationRef, type CodeLocationRef, type CodePosition } from "./codeNavigationApi";
import type { CodeQueryHandle, CodeQueryOutcome, CodeQueryResult, CodeSession } from "./useCodeSession";
import type { WorktreeFilesApi } from "./worktreeFilesApi";

export type SelectedFile = {
  relPath: string;
  content: string;
  error: boolean;
  /** One-based line the preview scrolls to and marks as the navigation target. */
  focusLine?: number;
};

/**
 * The hover card of the identifier at `at`, anchored to its element: `loading` while the host holds
 * the query for a loading index, then the markdown it answered.
 */
export type Hover = {
  at: CodePosition;
  relPath: string;
  anchor: HTMLElement;
  content: { kind: "loading" } | { kind: "markdown"; markdown: string };
};

/** A list of locations the user picks from — the references of a symbol, or ambiguous definitions. */
export type LocationList = { title: string; locations: CodeLocationRef[] };

/** The navigation notice: a held query waiting for the index, or an outcome to read. */
export type Notice = { kind: "held" } | { kind: "message"; text: string; isError: boolean };

const errorMessage = (e: unknown, fallback: string) => (e instanceof Error ? e.message : fallback);

const NOT_LOADED = "The code index has not loaded — nothing found yet";

/** A probe answer remembered per file and position: the definition's locations, or none. */
type ProbeCache = ReadonlyMap<string, CodeLocationRef[]>;

const probeKey = (relPath: string, at: CodePosition) => `${relPath}:${at.line}:${at.column}`;

/** The transient navigation UI: hover card, location list and notice. */
function useNavigationUi() {
  const [hover, setHover] = useState<Hover | null>(null);
  const [locations, setLocations] = useState<LocationList | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);
  return { hover, setHover, locations, setLocations, notice, setNotice };
}

type NavigationUi = ReturnType<typeof useNavigationUi>;

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
    (handle: CodeQueryHandle, onOutcome: (result: CodeQueryResult) => void, onError: (e: unknown) => void) => {
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

/** The file shown in the preview; opening one dismisses the navigation UI of the previous one. */
function useSelectedFile(api: WorktreeFilesApi, onOpen: () => void) {
  const [selected, setSelected] = useState<SelectedFile | null>(null);

  const openFile = useCallback(
    (relPath: string, focusLine?: number) => {
      onOpen();
      void api
        .readFile(relPath)
        .then((res) => setSelected({ relPath, content: res.contentUtf8, error: false, focusLine }))
        .catch((e: unknown) => {
          setSelected({ relPath, content: errorMessage(e, "Failed to read file"), error: true });
        });
    },
    [api, onOpen],
  );

  return { selected, openFile };
}

/**
 * Probes ("is there a definition here?") per file and position, so the preview can underline what a
 * click would go to. A probe the loading index could not answer (`UNKNOWN`) is not remembered, so
 * the next dwell asks again.
 */
function useProbes(session: CodeSession | null) {
  const [probes, setProbes] = useState<ProbeCache>(() => new Map());
  const asked = useRef(new Set<string>());

  const probe = useCallback(
    (relPath: string, at: CodePosition) => {
      const key = probeKey(relPath, at);
      if (session === null || probes.has(key) || asked.current.has(key)) return;
      asked.current.add(key);
      const settle = (locations: CodeLocationRef[] | null) => {
        asked.current.delete(key);
        if (locations !== null) setProbes((prev) => new Map(prev).set(key, locations));
      };
      session.ask(CodeQueryKind.PROBE, relPath, at).outcome.then(
        (outcome) => {
          if (outcome.kind !== "answered" || outcome.result.case !== "probe") return settle(null);
          const { definability, locations } = outcome.result.value;
          if (definability === Definability.DEFINABLE) return settle(locations.map(toLocationRef));
          settle(definability === Definability.NONE ? [] : null);
        },
        // A failed probe leaves the identifier plain; the session's failure is the strip's to show.
        () => settle(null),
      );
    },
    [session, probes],
  );

  const forget = useCallback(() => {
    asked.current.clear();
    setProbes(new Map());
  }, []);

  return { probes, probe, forget };
}

function useNavigationActions(
  session: CodeSession | null,
  selectedPath: string | null,
  openFile: (relPath: string, focusLine?: number) => void,
  ui: NavigationUi,
  probes: ProbeCache,
  queries: { hover: ReturnType<typeof useLatestQuery>; lookup: ReturnType<typeof useLatestQuery> },
) {
  const { hover, setHover, setLocations, setNotice } = ui;

  const showError = useCallback(
    (what: string) => (e: unknown) =>
      setNotice({
        kind: "message",
        text: `${what} failed: ${errorMessage(e, "unknown error")}`,
        isError: true,
      }),
    [setNotice],
  );

  const openLocation = useCallback(
    (location: CodeLocationRef) => {
      if (location.outsideWorktree) return;
      openFile(location.relPath, location.start.line);
    },
    [openFile],
  );

  const goToDefinitions = useCallback(
    (found: CodeLocationRef[]) => {
      if (found.length === 1) openLocation(found[0]);
      else setLocations({ title: "Definitions", locations: found });
    },
    [openLocation, setLocations],
  );

  const navigate = useCallback(
    (at: CodePosition) => {
      if (session === null || selectedPath === null) return;
      setNotice(null);
      const probed = probes.get(probeKey(selectedPath, at));
      if (probed !== undefined && probed.length > 0) {
        queries.lookup.cancel();
        goToDefinitions(probed);
        return;
      }
      const handle = session.ask(CodeQueryKind.DEFINITION, selectedPath, at, {
        onHeld: () => {
          if (queries.lookup.isLatest(handle.id)) setNotice({ kind: "held" });
        },
      });
      queries.lookup.track(
        handle,
        (result) => {
          setNotice(null);
          if (result.case === "error") return showError("Go to definition")(new Error(result.value));
          if (result.case !== "definition") return;
          const found = result.value.locations.map(toLocationRef);
          if (found.length > 0) return goToDefinitions(found);
          setNotice({
            kind: "message",
            text: result.value.state === CodeIndexState.READY ? "No definition found" : NOT_LOADED,
            isError: false,
          });
        },
        showError("Go to definition"),
      );
    },
    [session, selectedPath, probes, queries.lookup, goToDefinitions, setNotice, showError],
  );

  const showHover = useCallback(
    (at: CodePosition, anchor: HTMLElement) => {
      if (session === null || selectedPath === null) return;
      const relPath = selectedPath;
      // A card still loading belongs to the identifier the pointer left; its query is dropped below.
      setHover((prev) => (prev?.content.kind === "loading" ? null : prev));
      const handle = session.ask(CodeQueryKind.HOVER, relPath, at, {
        onHeld: () => {
          if (queries.hover.isLatest(handle.id)) {
            setHover({ at, relPath, anchor, content: { kind: "loading" } });
          }
        },
      });
      queries.hover.track(
        handle,
        (result) => {
          if (result.case === "error") {
            setHover(null);
            return showError("Hover")(new Error(result.value));
          }
          const markdown = result.case === "hover" ? result.value.markdown : undefined;
          setHover(
            markdown === undefined ? null : { at, relPath, anchor, content: { kind: "markdown", markdown } },
          );
        },
        (e) => {
          setHover(null);
          showError("Hover")(e);
        },
      );
    },
    [session, selectedPath, queries.hover, setHover, showError],
  );

  const showReferences = useCallback(() => {
    if (session === null || hover === null) return;
    setNotice(null);
    const handle = session.ask(CodeQueryKind.REFERENCES, hover.relPath, hover.at, {
      onHeld: () => {
        if (queries.lookup.isLatest(handle.id)) setNotice({ kind: "held" });
      },
    });
    queries.lookup.track(
      handle,
      (result) => {
        setNotice(null);
        if (result.case === "error") return showError("Find references")(new Error(result.value));
        if (result.case !== "references") return;
        const found = result.value.locations.map(toLocationRef);
        if (found.length === 0 && result.value.state !== CodeIndexState.READY) {
          return setNotice({ kind: "message", text: NOT_LOADED, isError: false });
        }
        setLocations({ title: "References", locations: found });
      },
      showError("Find references"),
    );
  }, [session, hover, queries.lookup, setNotice, setLocations, showError]);

  return { openLocation, navigate, showHover, showReferences };
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
  const hoverQuery = useLatestQuery(session);
  const lookupQuery = useLatestQuery(session);
  const { probes, probe, forget } = useProbes(session);
  const { setHover, setLocations, setNotice } = ui;

  const clear = useCallback(() => {
    hoverQuery.cancel();
    lookupQuery.cancel();
    // Positions are remembered against the content the preview showed; a newly read file may differ.
    forget();
    setHover(null);
    setLocations(null);
    setNotice(null);
  }, [hoverQuery, lookupQuery, forget, setHover, setLocations, setNotice]);

  const { selected, openFile } = useSelectedFile(api, clear);
  const selectedPath = selected && !selected.error ? selected.relPath : null;
  const queries = useMemo(() => ({ hover: hoverQuery, lookup: lookupQuery }), [hoverQuery, lookupQuery]);
  const actions = useNavigationActions(session, selectedPath, openFile, ui, probes, queries);

  const closeHover = useCallback(() => {
    hoverQuery.cancel();
    setHover(null);
  }, [hoverQuery, setHover]);

  /** The pointer dwelt on the identifier at `at`: probe it for an underline and ask its hover. */
  const dwell = useCallback(
    (at: CodePosition, anchor: HTMLElement) => {
      if (selectedPath === null) return;
      probe(selectedPath, at);
      actions.showHover(at, anchor);
    },
    [selectedPath, probe, actions],
  );

  /** Whether a probe found a definition for the identifier at `at` in the selected file. */
  const isDefinable = useCallback(
    (at: CodePosition) =>
      selectedPath !== null && (probes.get(probeKey(selectedPath, at))?.length ?? 0) > 0,
    [selectedPath, probes],
  );

  return {
    selected,
    openFile,
    hover: ui.hover,
    locations: ui.locations,
    notice: ui.notice,
    closeHover,
    dwell,
    isDefinable,
    openLocation: actions.openLocation,
    navigate: actions.navigate,
    showReferences: actions.showReferences,
  };
}
