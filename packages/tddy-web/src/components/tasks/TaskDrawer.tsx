import React from "react";
import type { TaskInfo } from "../../gen/tasks_pb";
import { ScrollArea } from "../ui/scroll-area";
import { TaskDrawerItem } from "./TaskDrawerItem";

interface TaskDrawerProps {
  tasks: TaskInfo[];
  selectedTaskId: string | null;
  onSelectTask: (taskId: string) => void;
  sessionToken: string;
}

export function TaskDrawer({ tasks, selectedTaskId, onSelectTask, sessionToken }: TaskDrawerProps) {
  const sorted = [...tasks].sort((a, b) => Number(b.createdUnixMs - a.createdUnixMs));

  return (
    <div
      data-testid="tasks-drawer"
      className="flex flex-col h-full border-r border-border bg-background"
      style={{ width: 280, flexShrink: 0 }}
    >
      <div className="px-3 py-2 border-b border-border">
        <span className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
          Tasks
        </span>
      </div>
      <ScrollArea className="flex-1 min-h-0">
        <div className="py-1 px-2 space-y-0.5">
          {sorted.map((task) => (
            <TaskDrawerItem
              key={task.taskId}
              task={task}
              isSelected={task.taskId === selectedTaskId}
              onClick={onSelectTask}
              sessionToken={sessionToken}
            />
          ))}
          {sorted.length === 0 && (
            <div className="px-3 py-4 text-sm text-muted-foreground text-center">
              No tasks
            </div>
          )}
        </div>
      </ScrollArea>
    </div>
  );
}
