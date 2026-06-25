import { useEffect, useRef, useState } from "react";
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import { TaskService } from "../../gen/tasks_pb";

function createTaskClient() {
  const transport = createConnectTransport({
    baseUrl: typeof window !== "undefined" ? `${window.location.origin}/rpc` : "",
    useBinaryFormat: true,
  });
  return createClient(TaskService, transport);
}

export function useTaskChannelStream(
  sessionToken: string,
  taskId: string | null,
  channelId: string | null
) {
  const [output, setOutput] = useState<string>("");
  const abortRef = useRef<AbortController | null>(null);
  const decoderRef = useRef(new TextDecoder());

  useEffect(() => {
    if (!taskId || channelId === null) {
      setOutput("");
      return;
    }

    setOutput("");
    const controller = new AbortController();
    abortRef.current = controller;
    const client = createTaskClient();

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
  }, [sessionToken, taskId, channelId]);

  return { output };
}
