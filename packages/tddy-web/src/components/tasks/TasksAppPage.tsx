import { useCallback, useEffect, useMemo, useState } from "react";
import { createClient } from "@connectrpc/connect";
import { TaskService, type TaskInfo } from "../../gen/tasks_pb";
import { useAuth } from "../../hooks/useAuth";
import { useHttpTransport } from "../../rpc/transportProvider";
import { DaemonNavMenu } from "../shell/DaemonNavMenu";
import { UserAvatar } from "../UserAvatar";
import { Button } from "../ui/button";

const screenShellClassName =
  "min-h-svh w-full min-w-0 box-border px-4 py-6 sm:px-6 font-sans text-foreground";

function statusLabel(status: number): string {
  switch (status) {
    case 1: return "Pending";
    case 2: return "Running";
    case 3: return "Completed";
    case 4: return "Failed";
    case 5: return "Cancelled";
    default: return "Unknown";
  }
}

function statusColor(status: number): string {
  switch (status) {
    case 2: return "text-blue-600 dark:text-blue-400";
    case 3: return "text-green-600 dark:text-green-400";
    case 4: return "text-red-600 dark:text-red-400";
    case 5: return "text-yellow-600 dark:text-yellow-400";
    default: return "text-muted-foreground";
  }
}

function relativeTime(ms: bigint): string {
  const nowMs = BigInt(Date.now());
  const diffSec = Number((nowMs - ms) / 1000n);
  if (diffSec < 60) return `${diffSec}s ago`;
  if (diffSec < 3600) return `${Math.floor(diffSec / 60)}m ago`;
  return `${Math.floor(diffSec / 3600)}h ago`;
}

export function TasksAppPage({ onNavigate }: { onNavigate: (path: string) => void }) {
  const { user, logout, sessionToken } = useAuth();
  const transport = useHttpTransport();
  const client = useMemo(() => createClient(TaskService, transport), [transport]);

  const [tasks, setTasks] = useState<TaskInfo[]>([]);
  const [error, setError] = useState("");
  const [cancelling, setCancelling] = useState<Set<string>>(new Set());

  const loadTasks = useCallback(() => {
    if (!sessionToken) return;
    client
      .listTasks({ sessionToken, daemonInstanceId: "" })
      .then((res) => setTasks(res.tasks))
      .catch((e: unknown) => setError(e instanceof Error ? e.message : "Failed to load tasks"));
  }, [client, sessionToken]);

  useEffect(() => {
    loadTasks();
    const interval = setInterval(loadTasks, 3000);
    return () => clearInterval(interval);
  }, [loadTasks]);

  const handleCancel = useCallback(
    (taskId: string) => {
      if (!sessionToken) return;
      setCancelling((prev) => new Set([...prev, taskId]));
      client
        .cancelTask({ sessionToken, taskId, daemonInstanceId: "" })
        .then(() => loadTasks())
        .catch(() => {})
        .finally(() => {
          setCancelling((prev) => {
            const next = new Set(prev);
            next.delete(taskId);
            return next;
          });
        });
    },
    [client, sessionToken, loadTasks]
  );

  const sorted = [...tasks].sort(
    (a, b) => Number(b.createdUnixMs - a.createdUnixMs)
  );

  return (
    <div className={screenShellClassName}>
      <div className="flex items-center gap-3 mb-6">
        <DaemonNavMenu onNavigate={onNavigate} />
        <h1 className="text-xl font-bold flex-1">Tasks</h1>
        <Button variant="outline" size="sm" onClick={loadTasks}>
          Refresh
        </Button>
        {user ? <UserAvatar user={user} onLogout={logout} /> : null}
      </div>

      {error ? (
        <p className="text-destructive text-sm mb-4">{error}</p>
      ) : null}

      {sorted.length === 0 ? (
        <p className="text-muted-foreground text-sm">No tasks yet.</p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-sm border-collapse">
            <thead>
              <tr className="border-b border-border text-left">
                <th className="pb-2 pr-4 font-medium text-muted-foreground">Kind</th>
                <th className="pb-2 pr-4 font-medium text-muted-foreground">Status</th>
                <th className="pb-2 pr-4 font-medium text-muted-foreground">Created</th>
                <th className="pb-2 font-medium text-muted-foreground">Actions</th>
              </tr>
            </thead>
            <tbody>
              {sorted.map((task) => {
                const isRunning = task.status === 2;
                const isPending = task.status === 1;
                const cancellable = isRunning || isPending;
                return (
                  <tr key={task.taskId} className="border-b border-border/50 hover:bg-muted/30">
                    <td className="py-2 pr-4 font-mono text-xs max-w-[280px] truncate" title={task.kind}>
                      {task.kind}
                    </td>
                    <td className={`py-2 pr-4 font-medium ${statusColor(task.status)}`}>
                      {statusLabel(task.status)}
                    </td>
                    <td className="py-2 pr-4 text-muted-foreground">
                      {relativeTime(task.createdUnixMs)}
                    </td>
                    <td className="py-2">
                      {cancellable ? (
                        <Button
                          variant="outline"
                          size="sm"
                          disabled={cancelling.has(task.taskId)}
                          onClick={() => handleCancel(task.taskId)}
                        >
                          {cancelling.has(task.taskId) ? "Cancelling…" : "Cancel"}
                        </Button>
                      ) : null}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
