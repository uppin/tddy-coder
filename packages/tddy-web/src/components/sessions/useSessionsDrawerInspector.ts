import { useCallback, useEffect } from "react";
import {
  parseSessionsDrawerSessionId,
  isSessionsNewPath,
  isInspectorTabName,
  type InspectorTabName,
} from "../../routing/appRoutes";
import { PARAM_FULL, PARAM_INSPECTOR } from "../../routing/appLocation";
import { useAppLocation } from "../../routing/useAppLocation";
import type { InspectorDrawerState } from "./SessionInspectorDrawer";
import { nextInspectorState } from "./inspectorState";

/**
 * The sessions drawer's URL-derived state: which session is selected, whether the screen is
 * creating one, and the inspector's tab and open/expanded state — plus the writer that puts an
 * inspector state back into the URL and the toggle/close/expand/restore handlers built on it.
 */
export function useSessionsDrawerInspector() {
  // Selection is derived from the URL, not held in state: that is what makes Back, Forward, a
  // pasted link and a reload all move the screen through the same code path.
  const { location, navigate, setParams } = useAppLocation();
  const selectedSessionId = parseSessionsDrawerSessionId(location.path);
  const mode: "list" | "creating" = isSessionsNewPath(location.path) ? "creating" : "list";

  // Inspector state, likewise: the tab's presence in the URL *is* "the inspector is open".
  const inspectorTabParam = location.params[PARAM_INSPECTOR] ?? "";
  const inspectorTab: InspectorTabName = isInspectorTabName(inspectorTabParam)
    ? inspectorTabParam
    : "details";
  const inspectorState: InspectorDrawerState =
    inspectorTabParam === ""
      ? "closed"
      : location.params[PARAM_FULL] === "1"
        ? "expanded"
        : "open";

  /**
   * Write an inspector state into the URL. `replace` for the transitions the screen makes on the
   * operator's behalf (the closed default on selection, the open on an attach error) so Back does
   * not step through states nobody chose; a plain push for a click.
   */
  const applyInspectorState = useCallback(
    (next: InspectorDrawerState, options?: { replace?: boolean; tab?: InspectorTabName }) => {
      const tab = options?.tab ?? inspectorTab;
      setParams(
        next === "closed"
          ? { [PARAM_INSPECTOR]: null, [PARAM_FULL]: null }
          : { [PARAM_INSPECTOR]: tab, [PARAM_FULL]: next === "expanded" ? "1" : null },
        options,
      );
    },
    [inspectorTab, setParams],
  );

  // A tab name the inspector does not have would render a blank panel — normalise it away.
  useEffect(() => {
    if (inspectorTabParam !== "" && !isInspectorTabName(inspectorTabParam)) {
      setParams({ [PARAM_INSPECTOR]: "details" }, { replace: true });
    }
  }, [inspectorTabParam, setParams]);

  const handleInspectorToggle = () => {
    const prevState = { open: inspectorState !== "closed", expanded: inspectorState === "expanded" };
    const next = nextInspectorState(prevState, { type: "toggle" });
    applyInspectorState(next.open ? (next.expanded ? "expanded" : "open") : "closed");
  };

  const handleInspectorClose = () => applyInspectorState("closed");
  const handleInspectorExpand = () => applyInspectorState("expanded");
  const handleInspectorRestore = () => applyInspectorState("open");

  return {
    location,
    navigate,
    setParams,
    selectedSessionId,
    mode,
    inspectorTabParam,
    inspectorState,
    applyInspectorState,
    handleInspectorToggle,
    handleInspectorClose,
    handleInspectorExpand,
    handleInspectorRestore,
  };
}
