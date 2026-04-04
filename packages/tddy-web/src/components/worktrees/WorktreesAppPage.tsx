import { useCallback, useEffect, useMemo, useState } from "react";
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import {
  ConnectionService,
  type EligibleDaemonEntry,
  type ProjectEntry,
  type WorktreeRow,
} from "../../gen/connection_pb";
import { GitHubLoginButton } from "../GitHubLoginButton";
import { UserAvatar } from "../UserAvatar";
import { DaemonNavMenu } from "../shell/DaemonNavMenu";
import { useAuth } from "../../hooks/useAuth";
import { WorktreesScreen, type WorktreesScreenMockRow } from "./WorktreesScreen";
import { Button } from "@/components/ui/button";

const screenShellClassName =
  "min-h-svh w-full min-w-0 box-border px-4 py-6 sm:px-6 font-sans text-foreground";

const selectClassName =
  "box-border min-w-[12rem] max-w-[24rem] rounded-md border border-input bg-background px-2 py-1.5 text-sm text-foreground shadow-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring";

function createConnectionClient() {
  const transport = createConnectTransport({
    baseUrl: typeof window !== "undefined" ? `${window.location.origin}/rpc` : "",
    useBinaryFormat: true,
  });
  return createClient(ConnectionService, transport);
}

function formatDiskBytes(n: bigint): string {
  const v = Number(n);
  if (!Number.isFinite(v) || v < 0) return "—";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let x = v;
  let i = 0;
  while (x >= 1024 && i < units.length - 1) {
    x /= 1024;
    i += 1;
  }
  const rounded = i === 0 ? Math.round(x) : Math.round(x * 10) / 10;
  return `${rounded} ${units[i]}`;
}

function rowFromRpc(w: WorktreeRow): WorktreesScreenMockRow {
  return {
    path: w.path,
    branch: w.branchLabel,
    sizeLabel: formatDiskBytes(w.diskBytes),
    changedFiles: w.changedFiles,
    linesAdded: Number(w.linesAdded),
    linesRemoved: Number(w.linesRemoved),
    stale: w.stale,
  };
}

/**
 * Full-page Worktrees view: lists worktrees for a selected project via the local daemon
 * (ConnectionService worktree RPCs are not routed to remote hosts yet).
 */
export function WorktreesAppPage({
  onNavigate,
}: {
  onNavigate: (path: string) => void;
}) {
  const { user, isAuthenticated, login, logout, sessionToken } = useAuth();
  const client = useMemo(() => createConnectionClient(), []);

  const [projects, setProjects] = useState<ProjectEntry[]>([]);
  const [daemons, setDaemons] = useState<EligibleDaemonEntry[]>([]);
  const [projectId, setProjectId] = useState("");
  const [daemonId, setDaemonId] = useState("");
  const [rows, setRows] = useState<WorktreesScreenMockRow[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loadingList, setLoadingList] = useState(false);
  const [loadingRefresh, setLoadingRefresh] = useState(false);

  const loadProjectsAndDaemons = useCallback(() => {
    if (!sessionToken) return;
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

  const fetchWorktrees = useCallback(
    async (refresh: boolean) => {
      if (!sessionToken || !projectId.trim()) {
        setRows([]);
        return;
      }
      setError(null);
      if (refresh) setLoadingRefresh(true);
      else setLoadingList(true);
      try {
        const res = await client.listWorktreesForProject({
          sessionToken,
          projectId: projectId.trim(),
          refresh,
        });
        setRows(res.worktrees.map(rowFromRpc));
      } catch (e) {
        setError(e instanceof Error ? e.message : "Failed to load worktrees");
        setRows([]);
      } finally {
        setLoadingList(false);
        setLoadingRefresh(false);
      }
    },
    [client, sessionToken, projectId],
  );

  useEffect(() => {
    if (!sessionToken || !projectId.trim()) {
      setRows([]);
      return;
    }
    void fetchWorktrees(false);
  }, [sessionToken, projectId, fetchWorktrees]);

  const handleDelete = async (path: string) => {
    if (!sessionToken || !projectId.trim()) return;
    setError(null);
    try {
      await client.removeWorktree({
        sessionToken,
        projectId: projectId.trim(),
        worktreePath: path,
      });
      await fetchWorktrees(false);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to remove worktree");
    }
  };

  if (!isAuthenticated) {
    return (
      <div className={screenShellClassName}>
        <h1 className="text-2xl font-semibold">tddy-web</h1>
        <p className="mb-4 text-sm text-muted-foreground">
          Sign in with GitHub to access the app.
        </p>
        <GitHubLoginButton onClick={login} />
      </div>
    );
  }

  const localDaemon = daemons.find((d) => d.isLocal);
  const worktreesHostNote =
    daemonId && localDaemon && daemonId !== localDaemon.instanceId
      ? "Worktree list and actions use the local daemon only; switch the host back to the local instance to manage worktrees."
      : null;

  return (
    <div className={screenShellClassName}>
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div className="flex min-w-0 flex-wrap items-center gap-3">
          <DaemonNavMenu onNavigate={onNavigate} />
          <h1 className="text-2xl font-semibold">Worktrees</h1>
        </div>
        {user ? <UserAvatar user={user} onLogout={logout} /> : null}
      </div>

      <p className="mt-4 max-w-2xl text-sm text-muted-foreground">
        Select a project to view git worktrees and cached size/diff stats. Use <strong>Refresh stats</strong> to
        re-run <code className="text-xs">git worktree list</code> and per-worktree diffs (expensive). Delete removes
        a secondary worktree only.
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
        <Button
          type="button"
          variant="secondary"
          data-testid="worktrees-refresh-stats"
          disabled={!sessionToken || !projectId.trim() || loadingRefresh}
          onClick={() => void fetchWorktrees(true)}
        >
          {loadingRefresh ? "Refreshing…" : "Refresh stats"}
        </Button>
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
        {loadingList && !loadingRefresh ? (
          <p className="text-sm text-muted-foreground" data-testid="worktrees-loading">
            Loading…
          </p>
        ) : null}
        <WorktreesScreen
          worktrees={rows}
          onConfirmDelete={(path) => void handleDelete(path)}
          emptyHint={
            projectId.trim() === ""
              ? "Select a project to list worktrees."
              : "No cached rows yet. Click Refresh stats to populate (or open this project from Connection after stats were refreshed)."
          }
        />
      </div>
    </div>
  );
}
