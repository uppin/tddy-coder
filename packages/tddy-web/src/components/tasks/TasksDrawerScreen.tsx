import React, { useState } from "react";
import { useAuthContext } from "../../hooks/authProvider";
import { TaskDrawer } from "./TaskDrawer";
import { TaskOutputPane } from "./TaskOutputPane";
import { useTaskListStream } from "./useTaskListStream";

export function TasksDrawerScreen() {
  const { sessionToken: authSessionToken } = useAuthContext();
  const sessionToken = authSessionToken ?? "";

  const { tasks } = useTaskListStream(sessionToken);
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);

  const selectedTask = selectedTaskId ? (tasks.get(selectedTaskId) ?? null) : null;

  return (
    <div
      data-testid="tasks-drawer-screen"
      className="flex h-screen w-full overflow-hidden font-sans text-foreground"
    >
      <TaskDrawer
        tasks={[...tasks.values()]}
        selectedTaskId={selectedTaskId}
        onSelectTask={setSelectedTaskId}
        sessionToken={sessionToken}
      />
      <TaskOutputPane task={selectedTask} sessionToken={sessionToken} />
    </div>
  );
}
