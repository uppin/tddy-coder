import { useEffect, useRef, useState } from "react";
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import { TaskService, type TaskInfo } from "../../gen/tasks_pb";

function createTaskClient() {
  const transport = createConnectTransport({
    baseUrl: typeof window !== "undefined" ? `${window.location.origin}/rpc` : "",
    useBinaryFormat: true,
  });
  return createClient(TaskService, transport);
}

export function useTaskListStream(sessionToken: string) {
  const [tasks, setTasks] = useState<Map<string, TaskInfo>>(new Map());
  const abortRef = useRef<AbortController | null>(null);

  useEffect(() => {
    const controller = new AbortController();
    abortRef.current = controller;
    const client = createTaskClient();

    (async () => {
      let retryDelay = 0;
      while (!controller.signal.aborted) {
        if (retryDelay > 0) {
          await new Promise<void>((resolve) => {
            const t = setTimeout(resolve, retryDelay);
            controller.signal.addEventListener("abort", () => { clearTimeout(t); resolve(); }, { once: true });
          });
          if (controller.signal.aborted) break;
        }
        try {
          const stream = client.watchTaskList(
            { sessionToken, daemonInstanceId: "" },
            { signal: controller.signal }
          );
          for await (const event of stream) {
            retryDelay = 0;
            if (event.event.case === "taskAdded" || event.event.case === "taskUpdated") {
              const task = event.event.value;
              setTasks((prev) => {
                const next = new Map(prev);
                next.set(task.taskId, task);
                return next;
              });
            } else if (event.event.case === "taskRemoved") {
              const taskId = event.event.value;
              setTasks((prev) => {
                const next = new Map(prev);
                next.delete(taskId);
                return next;
              });
            }
          }
          // Stream ended cleanly — reconnect immediately to stay live.
          retryDelay = 0;
        } catch (e) {
          if (e instanceof DOMException && e.name === "AbortError") break;
          console.debug("[useTaskListStream] error, reconnecting:", e);
          retryDelay = retryDelay === 0 ? 1000 : Math.min(retryDelay * 2, 30000);
        }
      }
    })();

    return () => {
      controller.abort();
    };
  }, [sessionToken]);

  return { tasks };
}
