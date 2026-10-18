import { useCallback } from "react";

import { CodeQueryKind } from "../../gen/code_navigation_pb";
import type { CodePosition } from "./codeNavigationApi";
import { useShowError, type NavigationContext } from "./navigationLookup";

/** The label a failed hover is reported under. */
const HOVER_LABEL = "Hover";

/**
 * Ask the hover of the identifier at a position of the selected file as the latest hover: the card
 * shows loading while the index holds it, then the markdown it answered (or closes on none).
 */
export function useHoverQuery({
  session,
  selectedPath,
  queries,
  ui: { setHover, setNotice },
}: NavigationContext) {
  const showError = useShowError(setNotice);

  return useCallback(
    (at: CodePosition) => {
      if (session === null || selectedPath === null) return;
      const relPath = selectedPath;
      // A card still loading belongs to the identifier the pointer left; its query is dropped below.
      setHover((prev) => (prev?.content.kind === "loading" ? null : prev));
      const handle = session.ask(CodeQueryKind.HOVER, relPath, at, {
        onHeld: () => {
          if (queries.hover.isLatest(handle.id)) {
            setHover({ at, relPath, content: { kind: "loading" } });
          }
        },
      });
      queries.hover.track(
        handle,
        (result) => {
          if (result.case === "error") {
            setHover(null);
            return showError(HOVER_LABEL)(new Error(result.value));
          }
          const markdown = result.case === "hover" ? result.value.markdown : undefined;
          setHover(
            markdown === undefined ? null : { at, relPath, content: { kind: "markdown", markdown } },
          );
        },
        (e) => {
          setHover(null);
          showError(HOVER_LABEL)(e);
        },
      );
    },
    [session, selectedPath, queries.hover, setHover, showError],
  );
}
