import { useMemo } from "react";
import { ActivityService } from "../../gen/activity_pb";
import { CatalogService } from "../../gen/catalog_pb";
import { CodeNavigationService } from "../../gen/code_navigation_pb";
import { ExecToolService } from "../../gen/exec_tools_pb";
import { PrStackService } from "../../gen/pr_stack_pb";
import { SessionAgentService } from "../../gen/session_agents_pb";
import { SessionFilesService } from "../../gen/session_files_pb";
import { SessionService, type SessionEntry } from "../../gen/session_pb";
import { TerminalSessionService } from "../../gen/terminal_session_pb";
import { WorktreeService } from "../../gen/worktree_pb";
import { useHostConnection } from "../../rpc/connections/registry";
import { useDaemonClientFor } from "../../rpc/selectedDaemon";
import { owningHostForSession } from "../../utils/crossHostSessions";

/**
 * The daemon that owns the selected session — cross-host interaction (connect, resume, delete,
 * terminate) must reach that daemon, not the selected one — and every client addressed at it.
 */
export function useSessionsDrawerOwningHost(
  selectedSession: SessionEntry | null,
  selectedInstanceId: string | null,
) {
  const selectedOwningHost = useMemo(
    () => (selectedSession ? owningHostForSession(selectedSession, selectedInstanceId ?? "") : null),
    [selectedSession, selectedInstanceId],
  );
  const activeClient = useDaemonClientFor(SessionService, selectedOwningHost);
  const activeWorktreeClient = useDaemonClientFor(WorktreeService, selectedOwningHost);
  const activeCodeNavigationClient = useDaemonClientFor(CodeNavigationService, selectedOwningHost);
  const activeTerminalClient = useDaemonClientFor(TerminalSessionService, selectedOwningHost);
  const activeSessionFilesClient = useDaemonClientFor(SessionFilesService, selectedOwningHost);
  const activeSessionAgentClient = useDaemonClientFor(SessionAgentService, selectedOwningHost);
  const activeActivityClient = useDaemonClientFor(ActivityService, selectedOwningHost);
  const activeCatalogClient = useDaemonClientFor(CatalogService, selectedOwningHost);
  const activeExecToolClient = useDaemonClientFor(ExecToolService, selectedOwningHost);
  const activePrStackClient = useDaemonClientFor(PrStackService, selectedOwningHost);
  // The same daemon as a connection rather than as a client: attaching a session opens a connection
  // on its host, and the spawned-child runtimes attach theirs on the same one.
  const activeHost = useHostConnection(selectedOwningHost);

  return {
    activeClient,
    activeWorktreeClient,
    activeCodeNavigationClient,
    activeTerminalClient,
    activeSessionFilesClient,
    activeSessionAgentClient,
    activeActivityClient,
    activeCatalogClient,
    activeExecToolClient,
    activePrStackClient,
    activeHost,
  };
}
