import { useCallback } from "react";
import type { Client } from "@connectrpc/connect";
import type { ProjectService } from "../../gen/project_pb";
import {
  parseSessionsProjectHomeSetupProjectId,
  sessionsProjectHomeSetupPath,
  SESSIONS_DRAWER_ROUTE,
} from "../../routing/appRoutes";
import { useAppLocation } from "../../routing/useAppLocation";

/**
 * The sessions drawer's home-agent setup pane: which project it is open for (read from
 * `/sessions/project-home/:projectId`), the moves onto and off it, and the `SetProjectHomeAgent`
 * call its picker makes on the selected host. A stored choice re-reads the registry
 * (`onStored`), so the pinned **main** entry and the pane's Start button both see it.
 */
export function useProjectHomeSetup(input: {
  projectClient: Client<typeof ProjectService> | null;
  sessionToken: string;
  selectedInstanceId: string;
  onStored: () => void;
}) {
  const { projectClient, sessionToken, selectedInstanceId, onStored } = input;
  const { location, navigate } = useAppLocation();
  const setupProjectId = parseSessionsProjectHomeSetupProjectId(location.path);

  // A pushed entry: the operator clicked "main", so Back returns them to what they were looking at.
  const open = useCallback(
    (projectId: string) => navigate(sessionsProjectHomeSetupPath(projectId)),
    [navigate],
  );
  const close = useCallback(() => navigate(SESSIONS_DRAWER_ROUTE), [navigate]);

  const setHomeAgent = useCallback(
    async (projectId: string, homeAgent: string): Promise<void> => {
      if (!projectClient) throw new Error("not connected to a daemon");
      await projectClient.setProjectHomeAgent({
        sessionToken,
        projectId,
        homeAgent,
        daemonInstanceId: selectedInstanceId,
      });
      onStored();
    },
    [projectClient, sessionToken, selectedInstanceId, onStored],
  );

  return { setupProjectId, open, close, setHomeAgent };
}
