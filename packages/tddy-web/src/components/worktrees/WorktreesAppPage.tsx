import { useCallback, useEffect, useState } from "react";
import {
  ConnectionService,
  type EligibleDaemonEntry,
  type ProjectEntry,
} from "../../gen/connection_pb";
import { GitHubLoginButton } from "../GitHubLoginButton";
import { AppShell } from "../shell/AppShell";
import { useAuthContext } from "../../hooks/authProvider";
import { useDaemonClient } from "../../rpc/selectedDaemon";
import { WorktreesScreen, type WorktreesScreenMockRow } from "./WorktreesScreen";
import { useWorktreeStatsStream } from "../../rpc/useWorktreeStatsStream";
import { formatLastCalculated, type WorktreeStatsRow } from "../../lib/worktreeSize";

const selectClassName =
  "box-border min-w-[12rem] max-w-[24rem] rounded-md border border-input bg-background px-2 py-1.5 text-sm text-foreground shadow-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring";

/** Map one streamed domain row to the presentational `WorktreesScreen` row shape. */
function toScreenRow(row: WorktreeStatsRow, nowMs: number): WorktreesScreenMockRow {
  return {
    path: row.path,
    branch: row.branch,
    status: row.status,
    sizeLabel: row.status === "cached" ? row.sizeLabel : undefined,
    lastCalculatedLabel: formatLastCalculated(row.calculatedAtUnixMs, nowMs),
    changedFiles: row.changedFiles,
    linesAdded: row.linesAdded,
    linesRemoved: row.linesRemoved,
  };
}

/**
 * Full-page Worktrees view: lists worktrees for a selected project via the local daemon
 * (ConnectionService worktree RPCs are not routed to remote hosts yet). Disk sizes stream in lazily
 * via `StreamWorktreeStats` — each worktree shows its size lifecycle (None / Calculating / Cached),
 * a per-row Calculate control, and a project-wide Recalculate-all.
 *
 * TODO(follow-up): migrate the Session Inspector's Worktree tab (`SessionWorktreeTab` /
 * `useSessionWorktreeStats`) off its 10-minute `ListWorktreesForProject` poll onto this same stream;
 * out of scope for this milestone (would break `SessionWorktreeTabAcceptance`).
 */
export function WorktreesAppPage({
  onNavigate,
}: {
  onNavigate: (path: string) => void;
}) {
  const { isAuthenticated, login, sessionToken } = useAuthContext();
  const client = useDaemonClient(ConnectionService);

  const [projects, setProjects] = useState<ProjectEntry[]>([]);
  const [daemons, setDaemons] = useState<EligibleDaemonEntry[]>([]);
  const [projectId, setProjectId] = useState("");
  const [daemonId, setDaemonId] = useState("");
  const [error, setError] = useState<string | null>(null);

  const { rows, recalculateAll, refresh, calculate } = useWorktreeStatsStream(projectId);

  const loadProjectsAndDaemons = useCallback(() => {
    if (!sessionToken || !client) return;
    client
      .listProjects({ sessionToken })
      .then((res) => setProjects(res.projects))
      .catch(() => setProjects([]));
    client
      .listEligibleDaemons({ sessionToken })
      .then((res) => setDaemons(res.daemons))
      .catch(() => setDaemons([]));
  }, [client, sessionToken]);

  useEffect(() => {
    if (!sessionToken || !isAuthenticated) return;
    loadProjectsAndDaemons();
  }, [sessionToken, isAuthenticated, loadProjectsAndDaemons]);

  useEffect(() => {
    if (projects.length === 0) {
      if (projectId !== "") setProjectId("");
      return;
    }
    const stillValid = projects.some((p) => p.projectId === projectId);
    if (!stillValid) {
      setProjectId(projects[0]?.projectId ?? "");
    }
  }, [projects, projectId]);

  useEffect(() => {
    const local = daemons.find((d) => d.isLocal);
    if (local) {
      setDaemonId(local.instanceId);
      return;
    }
    if (daemons.length > 0) {
      setDaemonId(daemons[0].instanceId);
    }
  }, [daemons]);

  const handleDelete = async (path: string) => {
    if (!sessionToken || !projectId.trim() || !client) return;
    setError(null);
    try {
      await client.removeWorktree({
        sessionToken,
        projectId: projectId.trim(),
        worktreePath: path,
      });
      refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to remove worktree");
    }
  };

  if (!isAuthenticated) {
    return (
      <AppShell title="Worktrees" onNavigate={onNavigate} variant="scroll">
        <p className="mb-4 text-sm text-muted-foreground">
          Sign in with GitHub to access the app.
        </p>
        <GitHubLoginButton onClick={login} />
      </AppShell>
    );
  }

  const localDaemon = daemons.find((d) => d.isLocal);
  const worktreesHostNote =
    daemonId && localDaemon && daemonId !== localDaemon.instanceId
      ? "Worktree list and actions use the local daemon only; switch the host back to the local instance to manage worktrees."
      : null;

  const nowMs = Date.now();
  const screenRows = rows.map((row) => toScreenRow(row, nowMs));

  return (
    <AppShell title="Worktrees" onNavigate={onNavigate} variant="scroll">
      <p className="max-w-2xl text-sm text-muted-foreground">
        Select a project to view git worktrees and their diff stats. Each worktree's on-disk size is
        calculated lazily and streamed in — use a row's <strong>Calculate</strong> to size one
        worktree, or <strong>Recalculate all</strong> to re-size the whole project. Delete removes a
        secondary worktree only.
      </p>

      <div className="mt-4 flex flex-wrap items-end gap-4">
        <div className="flex min-w-[10rem] flex-col gap-1">
          <label className="text-sm font-medium" htmlFor="worktrees-project">
            Project
          </label>
          <select
            id="worktrees-project"
            data-testid="worktrees-project-select"
            className={selectClassName}
            value={projectId}
            onChange={(e) => setProjectId(e.target.value)}
          >
            <option value="">—</option>
            {projects.map((p) => (
              <option key={p.projectId} value={p.projectId}>
                {p.name || p.projectId}
              </option>
            ))}
          </select>
        </div>
        <div className="flex min-w-[10rem] flex-col gap-1">
          <label className="text-sm font-medium" htmlFor="worktrees-host">
            Host (informational)
          </label>
          <select
            id="worktrees-host"
            data-testid="worktrees-host-select"
            className={selectClassName}
            value={daemonId}
            onChange={(e) => setDaemonId(e.target.value)}
          >
            {daemons.map((d) => (
              <option key={d.instanceId} value={d.instanceId}>
                {d.label || d.instanceId}
                {d.isLocal ? " (local)" : ""}
              </option>
            ))}
          </select>
        </div>
      </div>

      {worktreesHostNote ? (
        <p className="mt-3 max-w-2xl text-sm text-amber-600 dark:text-amber-500">{worktreesHostNote}</p>
      ) : null}

      {error ? (
        <p className="mt-3 text-sm text-destructive" data-testid="worktrees-error">
          {error}
        </p>
      ) : null}

      <div className="mt-6">
        <WorktreesScreen
          worktrees={screenRows}
          onConfirmDelete={(path) => void handleDelete(path)}
          onCalculate={(path) => calculate(path)}
          onRecalculateAll={() => recalculateAll()}
          emptyHint={
            projectId.trim() === ""
              ? "Select a project to list worktrees."
              : "No worktrees for this project yet."
          }
        />
      </div>
    </AppShell>
  );
}
