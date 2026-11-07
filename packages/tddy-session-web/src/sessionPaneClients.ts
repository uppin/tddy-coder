import type { useSessionsDrawerClients } from "./useSessionsDrawerClients";
import type { useSessionsDrawerOwningHost } from "./useSessionsDrawerOwningHost";

type DrawerClients = ReturnType<typeof useSessionsDrawerClients>;
type OwningHostClients = ReturnType<typeof useSessionsDrawerOwningHost>;

/**
 * The one client per service that `SessionMainPane` receives.
 *
 * While creating, a new session is created on the selected daemon, so the pane gets the selected
 * daemon's clients. Otherwise it gets the clients of the host that owns the selected session — cross-host
 * interaction must reach that daemon — falling back to the selected daemon's while the owning host's
 * client is not there yet. The project and accounts clients are always the selected daemon's, and code
 * navigation is always the owning host's.
 */
export function sessionPaneClients(
  creating: boolean,
  drawer: DrawerClients,
  owning: OwningHostClients,
) {
  const pick = <T>(selected: T | null, owned: T | null): T | undefined =>
    (creating ? selected : (owned ?? selected)) ?? undefined;
  return {
    client: pick(drawer.client, owning.activeClient),
    projectClient: drawer.projectClient ?? undefined,
    taskClient: pick(drawer.taskClient, owning.activeTaskClient),
    worktreeClient: pick(drawer.worktreeClient, owning.activeWorktreeClient),
    codeNavigationClient: owning.activeCodeNavigationClient ?? undefined,
    terminalClient: pick(drawer.terminalClient, owning.activeTerminalClient),
    sessionFilesClient: pick(drawer.sessionFilesClient, owning.activeSessionFilesClient),
    sessionAgentClient: pick(drawer.sessionAgentClient, owning.activeSessionAgentClient),
    activityClient: pick(drawer.activityClient, owning.activeActivityClient),
    accountsClient: drawer.accountsClient ?? undefined,
    catalogClient: pick(drawer.catalogClient, owning.activeCatalogClient),
    execToolClient: pick(drawer.execToolClient, owning.activeExecToolClient),
    prStackClient: pick(drawer.prStackClient, owning.activePrStackClient),
  };
}
