import { useCallback } from "react";
import {
  parseSessionsProjectHomeSetup,
  sessionsProjectHomeSetupPath,
  SESSIONS_DRAWER_ROUTE,
  type SessionsProjectHomeSetupRoute,
} from "../../routing/appRoutes";
import { useAppLocation } from "../../routing/useAppLocation";

/**
 * The sessions drawer's main setup pane: which project it is open for and whether it replaces a
 * main already set up (read from `/sessions/project-home/:projectId[/replace]`), and the moves onto
 * and off it.
 *
 * PRD: docs/ft/web/session-drawer.md
 */
export function useProjectHomeSetup(): {
  /** The open setup pane's route; `null` when the pane is not open. */
  setup: SessionsProjectHomeSetupRoute | null;
  open: (projectId: string, options?: { replacing?: boolean }) => void;
  close: () => void;
} {
  const { location, navigate } = useAppLocation();
  const setup = parseSessionsProjectHomeSetup(location.path);

  // A pushed entry: the operator clicked "main" (or Replace main), so Back returns them to what they
  // were looking at.
  const open = useCallback(
    (projectId: string, options: { replacing?: boolean } = {}) =>
      navigate(sessionsProjectHomeSetupPath(projectId, options)),
    [navigate],
  );
  const close = useCallback(() => navigate(SESSIONS_DRAWER_ROUTE), [navigate]);

  return { setup, open, close };
}
