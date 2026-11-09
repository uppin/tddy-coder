import React from "react";
import { TaskDrawer } from "./TaskDrawer";
import { TaskOutputPane } from "./TaskOutputPane";
import { useTaskListStream } from "./useTaskListStream";
import { parseTaskId, tasksPathForTask } from "./tasksRoutes";
import { useAppLocation, useAuth } from "tddy-client-web";

/** The Tasks screen's content — the task drawer and the selected task's output. The app page wraps it in the shell. */
export function TasksScreen() {
  const { sessionToken: authSessionToken } = useAuth();
  const sessionToken = authSessionToken ?? "";

  const { tasks } = useTaskListStream(sessionToken);

  // The selected task is `#/tasks/:taskId`, not component state: selecting a task is a navigation,
  // so Back steps through the trail and a shared link opens on the task it names.
  const { location, navigate } = useAppLocation();
  const selectedTaskId = parseTaskId(location.path);
  const selectedTask = selectedTaskId ? (tasks.get(selectedTaskId) ?? null) : null;

  return (
    <div data-testid="tasks-drawer-screen" className="flex flex-1 min-h-0 overflow-hidden">
      <TaskDrawer
        tasks={[...tasks.values()]}
        selectedTaskId={selectedTaskId}
        onSelectTask={(taskId) => navigate(tasksPathForTask(taskId))}
        sessionToken={sessionToken}
      />
      <TaskOutputPane task={selectedTask} sessionToken={sessionToken} />
    </div>
  );
}
