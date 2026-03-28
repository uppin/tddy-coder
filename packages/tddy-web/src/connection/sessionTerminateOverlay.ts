import type { Signal } from "../gen/connection_pb";

/** Structured trace for terminate/SIGINT flows when `trace` is true (e.g. `debugLogging`). */
export function logTddyMarker(
  markerId: string,
  scope: string,
  data: Record<string, unknown> = {},
  trace = false
): void {
  if (!trace) return;
  // eslint-disable-next-line no-console -- intentional structured trace when tracing enabled
  console.debug(
    JSON.stringify({ tddy: { marker_id: markerId, scope, data } })
  );
}

export type DelegateSignalSessionRpcOptions = {
  sessionToken: string;
  sessionId: string;
  signal: Signal;
  signalSession: (req: {
    sessionToken: string;
    sessionId: string;
    signal: Signal;
  }) => Promise<unknown>;
  /** When true, emit console debug/info for development (e.g. `debugLogging` or `import.meta.env.DEV`). */
  trace?: boolean;
};

/**
 * Delegates to Connect-RPC `ConnectionService.SignalSession` with the given signal.
 * Used by Connection Screen and unit tests; production fullscreen Terminate uses this via `handleSignalSession` + rethrow.
 */
export async function delegateSignalSessionRpc(
  args: DelegateSignalSessionRpcOptions
): Promise<void> {
  const trace = args.trace ?? false;
  if (trace) {
    console.debug("[tddy] delegateSignalSessionRpc: enter", {
      sessionId: args.sessionId,
      signal: args.signal,
    });
  }
  logTddyMarker(
    "M001",
    "connection/sessionTerminateOverlay.delegateSignalSessionRpc",
    { sessionId: args.sessionId },
    trace
  );
  if (trace) {
    console.info("[tddy] delegateSignalSessionRpc: calling SignalSession RPC", {
      sessionId: args.sessionId,
    });
  }
  await args.signalSession({
    sessionToken: args.sessionToken,
    sessionId: args.sessionId,
    signal: args.signal,
  });
  if (trace) {
    console.info("[tddy] delegateSignalSessionRpc: SignalSession RPC completed", {
      sessionId: args.sessionId,
    });
  }
}

/** Invoked when the user clicks the overlay Terminate control; awaits optional async parent work (e.g. Connect-RPC). */
export async function handleTerminateOverlayClick(
  onSessionTerminate?: () => void | Promise<void>,
  options?: { trace?: boolean }
): Promise<void> {
  const trace = options?.trace ?? false;
  if (trace) {
    console.debug("[tddy] handleTerminateOverlayClick: enter");
  }
  logTddyMarker(
    "M002",
    "GhosttyTerminalLiveKit.connectionOverlay.terminateClick",
    {},
    trace
  );
  await onSessionTerminate?.();
  if (trace) {
    console.info("[tddy] handleTerminateOverlayClick: callback finished");
  }
}

/**
 * Accessible name for the Terminate overlay control (daemon `SignalSession` SIGINT — not PTY ETX).
 */
export function buildTerminateOverlayAriaLabel(): string {
  return "Terminate: send SIGINT to the daemon-tracked session process (same as Connection Screen Interrupt)";
}
