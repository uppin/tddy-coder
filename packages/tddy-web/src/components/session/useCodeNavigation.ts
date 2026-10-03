import { useCallback, useRef, useState } from "react";

import type { CodeNavigationApi, CodeLocationRef, CodePosition } from "./codeNavigationApi";
import type { WorktreeFilesApi } from "./worktreeFilesApi";

export type SelectedFile = {
  relPath: string;
  content: string;
  error: boolean;
  /** One-based line the preview scrolls to and marks as the navigation target. */
  focusLine?: number;
};

/** The hover card: the markdown the language server returned for the identifier at `at`. */
export type Hover = { at: CodePosition; relPath: string; markdown: string };

/** A list of locations the user picks from — the references of a symbol, or ambiguous definitions. */
export type LocationList = { title: string; locations: CodeLocationRef[] };

export type Notice = { text: string; isError: boolean };

const errorMessage = (e: unknown, fallback: string) => (e instanceof Error ? e.message : fallback);

/** The transient navigation UI: hover card, location list and notice. */
function useNavigationUi() {
  const [hover, setHover] = useState<Hover | null>(null);
  const [locations, setLocations] = useState<LocationList | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);
  // Only the latest hover request may show its card: an older answer arriving late is dropped.
  const hoverRequest = useRef(0);

  const clear = useCallback(() => {
    hoverRequest.current += 1;
    setHover(null);
    setLocations(null);
    setNotice(null);
  }, []);

  return { hover, setHover, locations, setLocations, notice, setNotice, hoverRequest, clear };
}

type NavigationUi = ReturnType<typeof useNavigationUi>;

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

function useNavigationActions(
  navigation: CodeNavigationApi | null,
  selectedPath: string | null,
  openFile: (relPath: string, focusLine?: number) => void,
  ui: NavigationUi,
) {
  const { hover, setHover, setLocations, setNotice, hoverRequest } = ui;

  const showError = useCallback(
    (what: string) => (e: unknown) =>
      setNotice({ text: `${what} failed: ${errorMessage(e, "unknown error")}`, isError: true }),
    [setNotice],
  );

  const openLocation = useCallback(
    (location: CodeLocationRef) => {
      if (location.outsideWorktree) return;
      openFile(location.relPath, location.start.line);
    },
    [openFile],
  );

  const navigate = useCallback(
    (at: CodePosition) => {
      if (!navigation || selectedPath === null) return;
      setNotice(null);
      void navigation
        .definition(selectedPath, at)
        .then((found) => {
          if (found.length === 1) {
            openLocation(found[0]);
          } else if (found.length === 0) {
            setNotice({ text: "No definition found", isError: false });
          } else {
            setLocations({ title: "Definitions", locations: found });
          }
        })
        .catch(showError("Go to definition"));
    },
    [navigation, selectedPath, openLocation, setNotice, setLocations, showError],
  );

  const showHover = useCallback(
    (at: CodePosition) => {
      if (!navigation || selectedPath === null) return;
      const request = ++hoverRequest.current;
      void navigation
        .hover(selectedPath, at)
        .then((markdown) => {
          if (request !== hoverRequest.current) return;
          setHover(markdown === null ? null : { at, relPath: selectedPath, markdown });
        })
        .catch((e: unknown) => {
          if (request === hoverRequest.current) showError("Hover")(e);
        });
    },
    [navigation, selectedPath, hoverRequest, setHover, showError],
  );

  const showReferences = useCallback(() => {
    if (!navigation || hover === null) return;
    setNotice(null);
    void navigation
      .references(hover.relPath, hover.at)
      .then((found) => setLocations({ title: "References", locations: found }))
      .catch(showError("Find references"));
  }, [navigation, hover, setNotice, setLocations, showError]);

  return { openLocation, navigate, showHover, showReferences };
}

/**
 * The preview's selected file together with the navigation state and handlers (definition, hover,
 * references, opening a location) that act on it.
 */
export function useCodeNavigation(api: WorktreeFilesApi, navigation: CodeNavigationApi | null) {
  const ui = useNavigationUi();
  const { selected, openFile } = useSelectedFile(api, ui.clear);
  const selectedPath = selected && !selected.error ? selected.relPath : null;
  const actions = useNavigationActions(navigation, selectedPath, openFile, ui);
  const { setHover } = ui;
  const closeHover = useCallback(() => setHover(null), [setHover]);

  return {
    selected,
    openFile,
    hover: ui.hover,
    locations: ui.locations,
    notice: ui.notice,
    closeHover,
    ...actions,
  };
}
