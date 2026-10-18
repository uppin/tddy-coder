import { useCallback } from "react";

import { useLookup, type NavigationContext } from "./navigationLookup";

/** Find references of the identifier the hover card shows, listed for the user to pick from. */
export function useReferencesLookup(context: NavigationContext) {
  const {
    session,
    ui: { hover, setNotice, setLocations },
  } = context;
  const askLookup = useLookup(context);

  return useCallback(() => {
    if (session === null || hover === null) return;
    setNotice(null);
    askLookup("references", hover.relPath, hover.at, (found) =>
      setLocations({ title: "References", locations: found }),
    );
  }, [session, hover, setNotice, setLocations, askLookup]);
}
