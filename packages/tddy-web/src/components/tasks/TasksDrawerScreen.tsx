import React, { useState } from "react";
import { useAuthContext } from "../../hooks/authProvider";
import { AppShell } from "../shell/AppShell";
import { TaskDrawer } from "./TaskDrawer";
import { TaskOutputPane } from "./TaskOutputPane";
import { useTaskListStream } from "./useTaskListStream";

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
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);

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
          onSelectTask={setSelectedTaskId}
          sessionToken={sessionToken}
        />
        <TaskOutputPane task={selectedTask} sessionToken={sessionToken} />
      </div>
    </AppShell>
  );
}
