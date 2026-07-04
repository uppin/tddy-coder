import React, { useCallback, useState } from "react";
import { TaskService, TaskStatusProto, type TaskInfo } from "../../gen/tasks_pb";
import { useDaemonClient } from "../../rpc/selectedDaemon";
import { Button } from "../ui/button";

function statusDotColor(status: TaskStatusProto): string {
  switch (status) {
    case TaskStatusProto.TASK_STATUS_RUNNING:
      return "bg-blue-500";
    case TaskStatusProto.TASK_STATUS_COMPLETED:
      return "bg-green-500";
    case TaskStatusProto.TASK_STATUS_FAILED:
      return "bg-red-500";
    case TaskStatusProto.TASK_STATUS_CANCELLED:
      return "bg-yellow-500";
    default:
      return "bg-muted-foreground/50";
  }
}

function isCancellable(status: TaskStatusProto): boolean {
  return (
    status === TaskStatusProto.TASK_STATUS_RUNNING ||
    status === TaskStatusProto.TASK_STATUS_PENDING
  );
}

interface TaskDrawerItemProps {
  task: TaskInfo;
  isSelected: boolean;
  onClick: (taskId: string) => void;
  sessionToken: string;
}

export function TaskDrawerItem({ task, isSelected, onClick, sessionToken }: TaskDrawerItemProps) {
  const [cancelling, setCancelling] = useState(false);
  const client = useDaemonClient(TaskService);

  const handleCancel = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      if (!client) return;
      setCancelling(true);
      client
        .cancelTask({ sessionToken, taskId: task.taskId, daemonInstanceId: "" })
        .catch(() => setCancelling(false));
    },
    [client, sessionToken, task.taskId]
  );

  return (
    <div
      data-testid={`tasks-drawer-item-${task.taskId}`}
      className={`flex items-center gap-2 px-2 py-1.5 rounded cursor-pointer text-sm select-none ${
        isSelected ? "bg-accent" : "hover:bg-muted/50"
      }`}
      onClick={() => onClick(task.taskId)}
    >
      <span
        data-testid={`tasks-drawer-item-status-${task.taskId}`}
        className={`w-2 h-2 rounded-full flex-shrink-0 ${statusDotColor(task.status)}`}
      />
      <span
        data-testid={`tasks-drawer-item-kind-${task.taskId}`}
        className="flex-1 min-w-0 font-mono text-xs truncate"
      >
        {task.kind}
      </span>
      {isCancellable(task.status) && (
        <Button
          data-testid={`tasks-drawer-item-cancel-${task.taskId}`}
          variant="outline"
          size="sm"
          className="h-5 px-1.5 text-xs flex-shrink-0"
          disabled={cancelling}
          onClick={handleCancel}
        >
          {cancelling ? "…" : "×"}
        </Button>
      )}
    </div>
  );
}
