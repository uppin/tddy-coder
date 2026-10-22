import { useCallback, useEffect, useRef, useSyncExternalStore } from "react";
import type { Client } from "@connectrpc/connect";
import { ActivityService } from "../../gen/activity_pb";
import { ExecToolService } from "../../gen/exec_tools_pb";
import { joinQuotedPaths } from "../../lib/shellQuote";
import { SessionRuntimeRegistry, makeByteTap, type ByteDelta } from "./sessionRuntimeRegistry";

/**
 * The sessions drawer's per-session runtime registry, its reactive `runtimes` snapshot, and the
 * callbacks that read from or write into it on behalf of the connected session.
 */
export function useSessionsDrawerRuntimeRegistry(connectedSessionId: string | null) {
  // Per-session runtime registry: one entry per attached session, surviving focus switches.
  // The inspector reads byte counters + last-received from it for active sessions; inactive sessions
  // fall back to the daemon-sourced `SessionEntry` fields (req 5 dual source). Created once and kept
  // in a ref so the `SessionRuntimeRegistry` instance (and its cached `runtimes` snapshot) is stable
  // across renders — must be instantiated before any callback that touches it (buildSessionClient,
  // onSessionDisconnect).
  const runtimeRegistryRef = useRef<SessionRuntimeRegistry | null>(null);
  runtimeRegistryRef.current ??= new SessionRuntimeRegistry();
  const runtimeRegistry = runtimeRegistryRef.current;
  const runtimes = useSyncExternalStore(
    (listener) => runtimeRegistry.subscribe(listener),
    () => runtimeRegistry.runtimes,
    () => runtimeRegistry.runtimes,
  );

  // This screen is route-level, so any navigation away unmounts it and takes the registry ref with
  // it — and with the registry goes the last reference to every session connection it was holding.
  // Releasing them here is the only thing standing between "the operator opened the projects page"
  // and a joined LiveKit room plus a self-rescheduling token timer per session, alive until the tab
  // is closed.
  useEffect(() => () => runtimeRegistry.closeAll(), [runtimeRegistry]);

  // Session-scoped SessionService client — the one that reaches the session's own process rather
  // than the daemon that hosts it. Built LAZILY, only when the user actually invokes a session-scoped
  // RPC (ExecuteTool), so that lifecycle RPCs (Delete/Signal/Resume/Connect) stay daemon-direct.
  // `ClaimTerminalControl` is no longer one of them: the terminal RPCs left `SessionService` for
  // `terminal_session.TerminalSessionService`, and each runtime builds that client off its own
  // connection. The connection memoises the client per service,
  // so an unchanged route yields one stable client identity: this callback is invoked inline while
  // rendering, and consumers key stream effects on the client.
  //
  // Read from the runtime registry rather than from the attachment: the registry holds the
  // connections that are still open, so a session whose runtime has been evicted yields `null` here
  // — the answer the inspector's `buildSessionClient?.() ?? client` fallback already expects — in
  // place of a released connection that would refuse the call.
  const buildSessionExecToolClient = useCallback(
    (): Client<typeof ExecToolService> | null =>
      connectedSessionId
        ? (runtimeRegistry.get(connectedSessionId)?.connection?.clientFor(ExecToolService) ?? null)
        : null,
    [connectedSessionId, runtimeRegistry],
  );

  // The same session-scoped route, addressed at `activity.ActivityService`: `tddy-coder`'s session
  // participant serves the replay and activity families there since `#unbundle` node 7, so the
  // transcript still comes off the session's own process when it has one. A second builder rather
  // than a widened one because `clientFor` is per service — one callback can only return one
  // service's client.
  const buildSessionActivityClient = useCallback(
    (): Client<typeof ActivityService> | null =>
      connectedSessionId
        ? (runtimeRegistry.get(connectedSessionId)?.connection?.clientFor(ActivityService) ?? null)
        : null,
    [connectedSessionId, runtimeRegistry],
  );

  // Register a session's Agent-terminal text-insert (fired once its terminal mounts), so the
  // inspector's Files tab can insert an uploaded file's host path into the focused session's terminal
  // via a click/tap.
  const onSessionRegisterInsert = useCallback(
    (sessionId: string, insertInput: (text: string) => void) => {
      runtimeRegistry.setInsertInput(sessionId, insertInput);
    },
    [runtimeRegistry],
  );

  // Fold a session's terminal I/O bytes into its runtime counters as the terminal fires them (per
  // output chunk / input yield). The registry's `notify()` re-renders the screen (via
  // `useSyncExternalStore`), so the inspector's byte meter ticks live — even for a backgrounded session.
  const onSessionBytes = useCallback(
    (sessionId: string, delta: ByteDelta) => {
      makeByteTap(runtimeRegistry, sessionId)(delta);
    },
    [runtimeRegistry],
  );

  // Insert an uploaded file's host path (Files tab → Insert / tap) into the focused session's
  // terminal, shell-escaped exactly as a native terminal file-drag would type it. The Files tab
  // closes the inspector itself (via `onCloseInspector`), so this only performs the insert.
  const handleInsertPathIntoTerminal = (hostPath: string) => {
    const focusedId = runtimeRegistry.focusedSessionId;
    if (!focusedId) return;
    runtimeRegistry.get(focusedId)?.insertInput?.(joinQuotedPaths([hostPath]));
  };

  return {
    runtimeRegistry,
    runtimes,
    buildSessionExecToolClient,
    buildSessionActivityClient,
    onSessionRegisterInsert,
    onSessionBytes,
    handleInsertPathIntoTerminal,
  };
}
