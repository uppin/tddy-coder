import React, { useEffect, useRef } from "react";
import { useTaskChannelStream } from "./useTaskChannelStream";

interface TaskChannelOutputProps {
  taskId: string;
  channelId: string;
  sessionToken: string;
}

export function TaskChannelOutput({ taskId, channelId, sessionToken }: TaskChannelOutputProps) {
  const { output } = useTaskChannelStream(sessionToken, taskId, channelId);
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [output]);

  return (
    <div
      data-testid={`tasks-channel-output-${taskId}-${channelId}`}
      className="flex-1 overflow-y-auto bg-black text-green-400 font-mono text-xs p-3 whitespace-pre-wrap break-all"
    >
      {output}
      <div ref={bottomRef} />
    </div>
  );
}
