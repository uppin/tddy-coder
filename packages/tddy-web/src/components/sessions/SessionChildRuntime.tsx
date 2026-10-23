import React, { useEffect, useMemo } from "react";
import type { SessionEntry } from "../../gen/session_pb";
import type { HostConnection } from "../../rpc/connections/types";
import type { ToolShortcutDef } from "../../lib/toolShortcuts";
import { hostNotConnectedText } from "../../rpc/rpcFailure";
import { useHostLabel } from "../../rpc/selectedDaemon";
import { InlineError } from "../ui/inline-error";
import { SessionRuntime } from "./SessionRuntime";
import type { SessionAgentClient, TerminalClient } from "./sessionRuntimeProps";
import type { ByteDelta, SessionRuntimeState } from "./sessionRuntimeRegistry";
import { useSessionAttachment, type SessionAttachmentState } from "./useSessionAttachment";

export interface SessionChildRuntimeProps {
  /** The spawned child conversation's session id. */
  sessionId: string;
  /** True when this child pane is the visible/interactive one. */
  focused: boolean;
  sessionToken: string;
  client?: SessionAgentClient | null;
  /** Owning daemon `TerminalSessionService` client, inherited from the parent runtime (see
   *  `SessionRuntimeProps.terminalClient`). */
  terminalClient?: TerminalClient | null;
  /** The connection to the daemon that owns this child — the child attaches its own session over it. */
  host?: HostConnection | null;
  mobileShortcuts?: ToolShortcutDef[];
  /** Register this child's Agent-terminal text-insert (see `SessionRuntime.onSessionRegisterInsert`). */
  onSessionRegisterInsert?: (sessionId: string, insertInput: (text: string) => void) => void;
  /** Account this child's terminal I/O bytes to its own session id (see `SessionRuntime.onSessionBytes`). */
  onSessionBytes?: (sessionId: string, delta: ByteDelta) => void;
  /** Drop this child (its output stream ended) — removes the pane and returns focus to the parent. */
  onDisconnect?: (sessionId: string) => void;
  sessions?: ReadonlyArray<SessionEntry>;
}

/**
 * Why a child that has not attached is not showing: no wire to its host, or the daemon refused the
 * attach. `null` while the attach is merely still in flight.
 */
function childAttachError(
  host: HostConnection | null,
  attachment: SessionAttachmentState,
  ownerLabel: string,
): string | null {
  if (!host) return hostNotConnectedText(ownerLabel);
  if (attachment.status === "error") return attachment.error;
  return null;
}

/**
 * A spawned child conversation rendered inside its parent's runtime. It owns its own attachment —
 * attaching the child over `ConnectSession` the first time it is mounted (i.e. when its tab is
 * first selected) — and, once connected, renders a nested {@link SessionRuntime} for the child so
 * the child gets its own Agent + bash terminals (and, recursively, its own spawned conversations).
 */
export function SessionChildRuntime({
  sessionId,
  focused,
  sessionToken,
  client,
  terminalClient,
  host = null,
  mobileShortcuts,
  onSessionRegisterInsert,
  onSessionBytes,
  onDisconnect,
  sessions = [],
}: SessionChildRuntimeProps) {
  const { state: attachment, hint, connectSession } = useSessionAttachment();
  const hostLabel = useHostLabel();

  useEffect(() => {
    if (!host) return;
    // Never rejects: a refused attach is the attachment's `error` state, rendered below.
    void connectSession(sessionId, sessionToken, host);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [host, sessionId, sessionToken]);

  // A child's connection is this component's own — no registry holds it — so closing its tab has to
  // release it here. Without that, opening and closing a conversation would leave the room its
  // session was reached over joined for as long as the page stayed open.
  const connection = attachment.status === "connected" ? attachment.connection : null;
  useEffect(() => () => connection?.close(), [connection]);

  // Project the attachment into a `SessionRuntimeState` the nested runtime can render. Until the
  // child's `ConnectSession` resolves there is nothing to render yet.
  const runtime = useMemo<SessionRuntimeState | null>(
    () =>
      connection
        ? {
            sessionId,
            attached: true,
            connection,
            ...(hint ? { hint } : {}),
            bytesIn: 0,
            bytesOut: 0,
            lastDataReceivedAt: null,
          }
        : null,
    [connection, hint, sessionId],
  );

  if (!runtime) {
    // Nothing to render until the child attaches — but an attach that cannot be attempted (no wire to
    // the child's host) or that the daemon refused is said here, on the child's own pane, rather
    // than left as a blank one.
    const owner = sessions.find((s) => s.sessionId === sessionId)?.daemonInstanceId.trim() ?? "";
    return (
      <InlineError
        testId={`sessions-child-attach-error-${sessionId}`}
        message={childAttachError(host, attachment, hostLabel(owner))}
        className="p-4"
      />
    );
  }

  return (
    <SessionRuntime
      runtime={runtime}
      focused={focused}
      sessionToken={sessionToken}
      client={client}
      terminalClient={terminalClient}
      host={host}
      mobileShortcuts={mobileShortcuts}
      onSessionRegisterInsert={onSessionRegisterInsert}
      onSessionBytes={onSessionBytes}
      onSessionDisconnect={onDisconnect}
      sessions={sessions}
    />
  );
}
