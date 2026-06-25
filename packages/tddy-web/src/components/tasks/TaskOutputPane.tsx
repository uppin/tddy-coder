import React, { useState } from "react";
import type { TaskInfo } from "../../gen/tasks_pb";
import { TaskChannelOutput } from "./TaskChannelOutput";

interface TaskOutputPaneProps {
  task: TaskInfo | null;
  sessionToken: string;
}

export function TaskOutputPane({ task, sessionToken }: TaskOutputPaneProps) {
  const [activeChannelId, setActiveChannelId] = useState<string>("0");

  if (!task) {
    return (
      <div
        data-testid="tasks-output-pane-empty"
        className="flex-1 flex items-center justify-center text-muted-foreground text-sm"
      >
        Select a task to view output
      </div>
    );
  }

  const channels = task.channels ?? [];
  const effectiveChannelId = channels.find((c) => c.channelId === activeChannelId)
    ? activeChannelId
    : (channels[0]?.channelId ?? "0");

  return (
    <div
      data-testid="tasks-output-pane"
      className="flex-1 flex flex-col min-w-0 overflow-hidden"
    >
      {channels.length > 0 && (
        <div className="flex border-b border-border px-2 pt-2 gap-1">
          {channels.map((ch) => (
            <button
              key={ch.channelId}
              data-testid={`tasks-channel-tab-${task.taskId}-${ch.channelId}`}
              className={`px-3 py-1 text-xs rounded-t border ${
                ch.channelId === effectiveChannelId
                  ? "bg-background border-border border-b-transparent"
                  : "bg-muted border-transparent text-muted-foreground"
              }`}
              onClick={() => setActiveChannelId(ch.channelId)}
            >
              {ch.name}
            </button>
          ))}
        </div>
      )}
      <TaskChannelOutput
        taskId={task.taskId}
        channelId={effectiveChannelId}
        sessionToken={sessionToken}
      />
    </div>
  );
}
