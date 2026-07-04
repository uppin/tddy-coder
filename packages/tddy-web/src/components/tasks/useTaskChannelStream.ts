import { useEffect, useRef, useState } from "react";
import { TaskService } from "../../gen/tasks_pb";
import { useDaemonClient } from "../../rpc/selectedDaemon";

export function useTaskChannelStream(
  sessionToken: string,
  taskId: string | null,
  channelId: string | null
) {
  const client = useDaemonClient(TaskService);
  const [output, setOutput] = useState<string>("");
  const abortRef = useRef<AbortController | null>(null);
  const decoderRef = useRef(new TextDecoder());

  useEffect(() => {
    if (!taskId || channelId === null || !client) {
      setOutput("");
      return;
    }

    setOutput("");
    const controller = new AbortController();
    abortRef.current = controller;

    (async () => {
      try {
        const stream = client.watchTask(
          { sessionToken, taskId, channelId, daemonInstanceId: "" },
          { signal: controller.signal }
        );
        for await (const event of stream) {
          if (event.data.length > 0) {
            const text = decoderRef.current.decode(event.data, { stream: true });
            setOutput((prev) => prev + text);
          }
        }
      } catch (e) {
        if (!(e instanceof DOMException && e.name === "AbortError")) {
          console.debug("[useTaskChannelStream] error:", e);
        }
      }
    })();

    return () => {
      controller.abort();
    };
  }, [client, sessionToken, taskId, channelId]);

  return { output };
}
