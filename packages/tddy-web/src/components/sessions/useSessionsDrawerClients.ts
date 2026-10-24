import { useDaemonClient } from "../../rpc/selectedDaemon";
import { ProjectService } from "../../gen/project_pb";
import { SessionService } from "../../gen/session_pb";
import { ActivityService } from "../../gen/activity_pb";
import { AccountsService } from "../../gen/accounts_pb";
import { CatalogService } from "../../gen/catalog_pb";
import { ExecToolService } from "../../gen/exec_tools_pb";
import { PrStackService } from "../../gen/pr_stack_pb";
import { SessionAgentService } from "../../gen/session_agents_pb";
import { SessionFilesService } from "../../gen/session_files_pb";
import { TerminalSessionService } from "../../gen/terminal_session_pb";
import { TaskService } from "../../gen/tasks_pb";
import { WorktreeService } from "../../gen/worktree_pb";

/** The sessions drawer's per-service clients on the selected daemon, each `null` until one is reachable. */
export function useSessionsDrawerClients() {
  // SessionService is daemon-level RPC — routed to whichever daemon is currently selected over
  // whichever wire reaches it (see `SelectedDaemonProvider` and `rpc/connections`).
  // `null` until a daemon is selected and a wire reaches it; every call site guards.
  // The selected-daemon `client` still owns the CREATE flow (a new session is created on the
  // selected host); cross-host interaction routes through `activeClient` (the screen's owning-host routing).
  const client = useDaemonClient(SessionService);
  const projectClient = useDaemonClient(ProjectService);
  // The worktree RPCs left `SessionService` for `worktree.WorktreeService`, so they need their
  // own client — the same daemon, over the same wire, addressed under the service that now serves
  // them. It follows `client`'s routing exactly (selected daemon here, owning host in
  // `activeWorktreeClient` in the screen) so the pane and its worktree reads cannot disagree about which
  // host they are talking to.
  const worktreeClient = useDaemonClient(WorktreeService);
  // Likewise for the nine terminal RPCs, which left `SessionService` for
  // `terminal_session.TerminalSessionService`. Same daemon, same wire, and the same routing as
  // `client` — a runtime's control lease and its terminal I/O must reach the host the session's
  // PTY actually lives on.
  const terminalClient = useDaemonClient(TerminalSessionService);
  // And for the thirteen file RPCs, which left it for `session_files.SessionFilesService`: the
  // inspector's Files tab and the create form's attachment staging.
  const sessionFilesClient = useDaemonClient(SessionFilesService);
  // `#unbundle` node 7 took two more families out. The roster and conversation RPCs went to
  // `session_agents.SessionAgentService` — the Add-agent flow and every conversation pane — and
  // activity, notifications and ACP replay to `activity.ActivityService`. Same daemon, same wire
  // and the same routing as `client`, for the same reason the three above follow it.
  const sessionAgentClient = useDaemonClient(SessionAgentService);
  const activityClient = useDaemonClient(ActivityService);
  // `#unbundle` node 8 took families A, L and P to their own services on the same daemon and wire.
  const catalogClient = useDaemonClient(CatalogService);
  // The signed-in person's provider accounts live in this daemon's vault; the New-session form lists
  // them for the harness it is about to start.
  const accountsClient = useDaemonClient(AccountsService);
  const execToolClient = useDaemonClient(ExecToolService);
  const prStackClient = useDaemonClient(PrStackService);
  // A start in progress is a task on the host doing it; the create form cancels it through this.
  const taskClient = useDaemonClient(TaskService);

  return {
    client,
    projectClient,
    worktreeClient,
    terminalClient,
    sessionFilesClient,
    sessionAgentClient,
    activityClient,
    catalogClient,
    accountsClient,
    execToolClient,
    prStackClient,
    taskClient,
  };
}
