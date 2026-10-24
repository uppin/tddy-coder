import { useCallback, useEffect, useMemo, useState } from "react";
import { type ProjectEntry, ProjectService } from "../../gen/project_pb";
import { WorktreeService } from "../../gen/worktree_pb";
import { GitHubLoginButton } from "../GitHubLoginButton";
import { AppShell } from "../shell/AppShell";
import { SessionWorktreeTab } from "../sessions/SessionWorktreeTab";
import { ScrollArea } from "../ui/scroll-area";
import { useAuthContext } from "../../hooks/authProvider";
import { useDaemonClient } from "../../rpc/selectedDaemon";
import { WorktreesScreen } from "./WorktreesScreen";
import { useWorktreeStatsStream } from "../../rpc/useWorktreeStatsStream";
import {
  mergeWorktreeItems,
  readRememberedWorktrees,
  sortWorktreesBySize,
  writeRememberedWorktrees,
} from "../../lib/worktreeList";
import { PARAM_PROJECT } from "../../routing/appLocation";
import { useAppLocation } from "../../routing/useAppLocation";

/**
 * Full-page Worktrees view, laid out like the Sessions screen: a side panel of the selected
 * project's worktrees — by name, biggest on-disk size first — and the selected worktree's pane (the
 * Session Inspector's Worktree tab) beside it.
 *
 * The list is on screen as soon as the page opens: the worktrees and sizes remembered from the last
 * visit show first (marked stale), then `StreamWorktreeStats` supplies the daemon's current list and
 * its persisted sizes, and sizes it has never computed are calculated on the spot, with progress
 * above the list. Refresh recalculates every worktree.
 *
 * WorktreeService RPCs are not routed to remote hosts yet, so the list is the local daemon's.
 */
export function WorktreesAppPage({
  onNavigate,
}: {
  onNavigate: (path: string) => void;
}) {
  const { isAuthenticated, login, sessionToken } = useAuthContext();
  const projectClient = useDaemonClient(ProjectService);
  const worktreeClient = useDaemonClient(WorktreeService);

  const [projects, setProjects] = useState<ProjectEntry[]>([]);
  // The project filter is `?project=` — "the worktrees of project X" is a destination worth linking to.
  const { location, setParams } = useAppLocation();
  const projectId = location.params[PARAM_PROJECT] ?? "";
  const setProjectId = useCallback(
    (next: string, options?: { replace?: boolean }) =>
      setParams({ [PARAM_PROJECT]: next === "" ? null : next }, options),
    [setParams],
  );
  const [selectedPath, setSelectedPath] = useState<string | null>(null);

  const { rows, recalculateAll, refresh } = useWorktreeStatsStream(projectId);

  useEffect(() => {
    if (!sessionToken || !isAuthenticated) return;
    projectClient
      ?.listProjects({ sessionToken })
      .then((res) => setProjects(res.projects))
      .catch(() => setProjects([]));
  }, [projectClient, sessionToken, isAuthenticated]);

  // Resolve the filter against the loaded projects and write the answer back: a URL naming a
  // project that is not registered (or none at all) settles on the first listed one, so the address
  // bar always names the project actually being shown. `replace` — the app resolved this, not the
  // operator.
  useEffect(() => {
    // An empty list is "not loaded yet", never "no projects exist" — clearing the param here would
    // wipe a deep-linked project during the `ListProjects` round-trip, before it could be matched.
    if (projects.length === 0) return;
    if (projects.some((p) => p.projectId === projectId)) return;
    setProjectId(projects[0]?.projectId ?? "", { replace: true });
  }, [projects, projectId, setProjectId]);

  // What the last visit saw, read once per project — the list this page opens on.
  const [remembered, setRemembered] = useState(() => readRememberedWorktrees(projectId));
  useEffect(() => {
    setRemembered(readRememberedWorktrees(projectId));
    setSelectedPath(null);
  }, [projectId]);

  const items = useMemo(
    () => sortWorktreesBySize(mergeWorktreeItems(rows, remembered)),
    [rows, remembered],
  );

  // Remember the sizes the daemon has answered, for the next visit.
  useEffect(() => {
    if (projectId === "" || rows.length === 0) return;
    writeRememberedWorktrees(projectId, items);
  }, [projectId, rows, items]);

  // The pane always shows a worktree: the biggest one until the operator picks another, and the
  // next biggest when the shown one is deleted.
  const selected = items.find((i) => i.path === selectedPath) ?? items[0] ?? null;

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

  return (
    <AppShell title="Worktrees" onNavigate={onNavigate} variant="fullbleed" requiresHost>
      <WorktreesScreen
        projects={projects}
        projectId={projectId}
        onSelectProject={setProjectId}
        items={items}
        selectedPath={selected?.path ?? null}
        onSelect={setSelectedPath}
        onRefresh={recalculateAll}
        emptyHint={
          projectId.trim() === ""
            ? "Select a project to list worktrees."
            : "No worktrees for this project yet."
        }
      >
        {selected ? (
          <>
            <div className="px-3 py-2 border-b border-border">
              <div data-testid="worktrees-selected-name" className="text-sm font-semibold truncate">
                {selected.name}
              </div>
              <div className="text-xs text-muted-foreground truncate">{selected.path}</div>
            </div>
            <ScrollArea className="flex-1 min-h-0">
              <SessionWorktreeTab
                key={selected.path}
                client={worktreeClient ?? null}
                sessionToken={sessionToken ?? ""}
                projectId={projectId}
                repoPath={selected.path}
                onDeleted={() => {
                  setSelectedPath(null);
                  refresh();
                }}
              />
            </ScrollArea>
          </>
        ) : null}
      </WorktreesScreen>
    </AppShell>
  );
}
