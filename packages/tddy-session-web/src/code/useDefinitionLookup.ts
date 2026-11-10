import { useCallback } from "react";

import type { CodeLocationRef, CodePosition } from "./codeNavigationApi";
import { infoNotice, useLookup, type NavigationContext } from "./navigationLookup";

/**
 * Go to definition of the identifier at a position of the selected file — straight from a probe's
 * answer when one found it, else asked as the latest lookup — and opening a location.
 */
export function useDefinitionLookup(context: NavigationContext) {
  const {
    session,
    selectedPath,
    openFile,
    probes,
    queries,
    ui: { setNotice, setLocations },
  } = context;
  const askLookup = useLookup(context);

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
      const probed = probes?.definitionsAt(selectedPath, at);
      if (probed !== undefined && probed.length > 0) {
        queries.lookup.cancel();
        goToDefinitions(probed);
        return;
      }
      askLookup("definition", selectedPath, at, (found) =>
        found.length > 0 ? goToDefinitions(found) : setNotice(infoNotice("No definition found")),
      );
    },
    [session, selectedPath, probes, queries.lookup, goToDefinitions, setNotice, askLookup],
  );

  return { openLocation, navigate };
}
