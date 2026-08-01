import React from "react";
import { useAuthContext } from "../../hooks/authProvider";
import { AppShell } from "../shell/AppShell";
import { TaskDrawer } from "./TaskDrawer";
import { TaskOutputPane } from "./TaskOutputPane";
import { useTaskListStream } from "./useTaskListStream";
import { parseTaskId, tasksPathForTask } from "../../routing/appRoutes";
import { useAppLocation } from "../../routing/useAppLocation";

export function TasksDrawerScreen({
  // Optional so isolated component tests can mount the screen without a router; production
  // (index.tsx) always wires the hash-router navigate.
  onNavigate = () => {},
}: {
  onNavigate?: (path: string) => void;
}) {
  const { sessionToken: authSessionToken } = useAuthContext();
  const sessionToken = authSessionToken ?? "";

  const { tasks } = useTaskListStream(sessionToken);

  // The selected task is `#/tasks/:taskId`, not component state: selecting a task is a navigation,
  // so Back steps through the trail and a shared link opens on the task it names.
  const { location, navigate } = useAppLocation();
  const selectedTaskId = parseTaskId(location.path);
  const selectedTask = selectedTaskId ? (tasks.get(selectedTaskId) ?? null) : null;

  return (
    <AppShell
      variant="fullbleed"
      title="Tasks"
      onNavigate={onNavigate}
      dataTestId="tasks-drawer-screen"
    >
      <div className="flex flex-1 min-h-0 overflow-hidden">
        <TaskDrawer
          tasks={[...tasks.values()]}
          selectedTaskId={selectedTaskId}
          onSelectTask={(taskId) => navigate(tasksPathForTask(taskId))}
          sessionToken={sessionToken}
        />
        <TaskOutputPane task={selectedTask} sessionToken={sessionToken} />
      </div>
    </AppShell>
  );
}
