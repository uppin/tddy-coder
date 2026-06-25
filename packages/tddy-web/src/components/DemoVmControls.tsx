import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createClient } from "@connectrpc/connect";
import {
  ConnectionService,
  DemoVmState,
} from "../gen/connection_pb";
import { useHttpTransport } from "../rpc/transportProvider";
import { Button } from "@/components/ui/button";

type VmStatus = {
  state: DemoVmState;
  shareUrl: string;
  message: string;
};

/**
 * Shows "Launch Demo VM" / "Stop Demo VM" controls and a share link for a session that
 * is in the demo phase (workflowGoal === "demo").
 */
export function DemoVmControls({
  sessionId,
  sessionToken,
}: {
  sessionId: string;
  sessionToken: string;
}) {
  const [status, setStatus] = useState<VmStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const transport = useHttpTransport();
  const client = useMemo(() => createClient(ConnectionService, transport), [transport]);

  const fetchStatus = useCallback(async () => {
    try {
      const res = await client.getDemoVmStatus({ sessionToken, sessionId });
      setStatus({
        state: res.state,
        shareUrl: res.shareUrl,
        message: res.message,
      });
    } catch {
      // silently ignore transient network errors during polling
    }
  }, [client, sessionId, sessionToken]);

  useEffect(() => {
    void fetchStatus();
    pollRef.current = setInterval(() => void fetchStatus(), 3000);
    return () => {
      if (pollRef.current) clearInterval(pollRef.current);
    };
  }, [fetchStatus]);

  const handleLaunch = async () => {
    setBusy(true);
    try {
      await client.startDemoVm({ sessionToken, sessionId });
      await fetchStatus();
    } catch (e) {
      console.error("[DemoVmControls] startDemoVm failed", e);
    } finally {
      setBusy(false);
    }
  };

  const handleStop = async () => {
    setBusy(true);
    try {
      await client.stopDemoVm({ sessionToken, sessionId });
      await fetchStatus();
    } catch (e) {
      console.error("[DemoVmControls] stopDemoVm failed", e);
    } finally {
      setBusy(false);
    }
  };

  const state = status?.state ?? DemoVmState.UNKNOWN;
  const isRunning = state === DemoVmState.RUNNING;
  const isBooting = state === DemoVmState.BOOTING;
  const isStopped =
    state === DemoVmState.STOPPED ||
    state === DemoVmState.UNKNOWN;

  return (
    <span className="inline-flex flex-wrap items-center gap-2" data-testid={`demo-vm-controls-${sessionId}`}>
      {isStopped && (
        <Button
          type="button"
          size="sm"
          disabled={busy}
          onClick={() => void handleLaunch()}
          data-testid={`demo-vm-launch-${sessionId}`}
        >
          Launch Demo VM
        </Button>
      )}
      {isBooting && (
        <span
          className="inline-flex shrink-0 items-center rounded border border-blue-300 bg-blue-50 px-1.5 py-0.5 text-xs font-medium text-blue-950"
          data-testid={`demo-vm-booting-${sessionId}`}
        >
          VM booting…
        </span>
      )}
      {isRunning && (
        <>
          <span
            className="inline-flex shrink-0 items-center rounded border border-green-300 bg-green-50 px-1.5 py-0.5 text-xs font-medium text-green-950"
            data-testid={`demo-vm-running-${sessionId}`}
          >
            VM running
          </span>
          {status?.shareUrl && (
            <a
              href={status.shareUrl}
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex shrink-0 items-center rounded border border-slate-300 bg-slate-50 px-1.5 py-0.5 text-xs font-medium text-slate-900 hover:bg-slate-100"
              data-testid={`demo-vm-share-url-${sessionId}`}
            >
              Open demo
            </a>
          )}
          <Button
            type="button"
            size="sm"
            variant="outline"
            disabled={busy}
            onClick={() => void handleStop()}
            data-testid={`demo-vm-stop-${sessionId}`}
          >
            Stop VM
          </Button>
        </>
      )}
      {state === DemoVmState.ERROR && (
        <>
          <span
            className="inline-flex shrink-0 items-center rounded border border-red-300 bg-red-50 px-1.5 py-0.5 text-xs font-medium text-red-950"
            title={status?.message}
            data-testid={`demo-vm-error-${sessionId}`}
          >
            VM error
          </span>
          <Button
            type="button"
            size="sm"
            disabled={busy}
            onClick={() => void handleLaunch()}
            data-testid={`demo-vm-retry-${sessionId}`}
          >
            Retry
          </Button>
        </>
      )}
    </span>
  );
}
