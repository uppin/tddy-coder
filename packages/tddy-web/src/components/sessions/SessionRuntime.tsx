import React, { useCallback, useMemo, useRef, useState } from "react";
import { createClient, type Client, type Transport } from "@connectrpc/connect";
import type { Room } from "livekit-client";
import { ConnectionService } from "../../gen/connection_pb";
import type { TokenService } from "../../gen/token_pb";
import { SessionLiveKitTerminal } from "./SessionLiveKitTerminal";
import { GrpcSessionTerminal } from "./GrpcSessionTerminal";
import { SessionTerminalTabs } from "./SessionTerminalTabs";
import { AGENT_TERMINAL_ID, useSessionTerminals } from "./useSessionTerminals";
import { TerminalControlOverlay } from "./TerminalControlOverlay";
import { useTerminalControl, type Session } from "./useTerminalControl";
import type { SessionRuntimeState } from "./sessionRuntimeRegistry";
import type { ToolShortcutDef } from "../../lib/toolShortcuts";
import { cn } from "../../lib/utils";

type ConnectionClient = Client<typeof ConnectionService>;
type TokenClient = Client<typeof TokenService>;

export interface SessionRuntimeProps {
  /** This runtime's attached-session state (connection params + status). */
  runtime: SessionRuntimeState;
  /** True when this runtime is the focused (CSS-visible) one. Drives the control overlay + mobile
   *  shortcut overlay; backgrounded runtimes stay mounted but `display:none`. */
  focused: boolean;
  sessionToken: string;
  /** Owning daemon `ConnectionService` client — used for gRPC terminal I/O and as the fallback for
   *  the auto-claim-on-attach. Pass `null`/`undefined` until the owning daemon is reachable. */
  client?: ConnectionClient | null;
  /** Browser LiveKit-token client — required to render a LiveKit terminal. */
  tokenClient?: TokenClient;
  /** Shortcut presets — shown as the mobile shortcut overlay on the focused runtime only. */
  mobileShortcuts?: ToolShortcutDef[];
  /** Capture the runtime's connected LiveKit `Room` (for session-scoped RPC routing). */
  onSessionRoom?: (sessionId: string, room: Room) => void;
  /** Evict this runtime's terminal (e.g. remote session ended). */
  onSessionDisconnect?: (sessionId: string) => void;
  /** LiveKit transport factory — builds the session-scoped client transport for the explicit
   *  steal-claim (`ClaimTerminalControl`, session-participant routing) and, for `connected-livekit`
   *  sessions, the bash terminals' I/O. */
  liveKitFactory?: (room: Room, targetIdentity: string) => Transport;
  /** True when `liveKitFactory` is a test double that ignores its `room` argument — the common
   *  room is then an acceptable stand-in for the session's own room. */
  liveKitFactoryIsOverridden?: boolean;
  /** Shared common room — used as the session-room stand-in when the factory is overridden. */
  commonRoom?: Room | null;
}

/**
 * One attached session's runtime: a terminal tab bar (Agent + bash terminals) over a stack of
 * mounted terminal panes, plus this session's own terminal-control lease.
 *
 * Each runtime owns its `useTerminalControl` hook — and therefore its own `connected` lease state —
 * so switching focus between sessions can never leak one session's control token into another
 * session's terminal input (the root cause of the "terminal controlled by another screen" failures
 * on fast session change). Every terminal of the session (Agent + bash) shares that one lease.
 *
 * All of the session's terminals stay mounted simultaneously — the active one is CSS-visible, the
 * others are `display:none` but keep streaming — so switching tabs (or backgrounding the whole
 * session) never tears a terminal down. The focused runtime additionally carries the
 * `sessions-detail-terminal-container` marker and the `TerminalControlOverlay`.
 *
 * Feature: `docs/ft/web/session-terminal-tabs.md`, `docs/ft/web/session-drawer.md#fast-session-change`.
 */
export function SessionRuntime({
  runtime,
  focused,
  sessionToken,
  client,
  tokenClient,
  mobileShortcuts,
  onSessionRoom,
  onSessionDisconnect,
  liveKitFactory,
  liveKitFactoryIsOverridden = false,
  commonRoom = null,
}: SessionRuntimeProps) {
  // The runtime's own connected Room, captured via the terminal's `onRoom`. Stored both in a ref
  // (for the lazy steal-claim client) and in state (so the memoized session-scoped terminal client
  // below rebuilds once the room connects).
  const roomRef = useRef<Room | null>(null);
  const [sessionRoom, setSessionRoom] = useState<Room | null>(null);

  // Lazy session-scoped ConnectionService client (targets the coder participant
  // `daemon-{instanceId}-{sessionId}` = `runtime.livekitServerIdentity`). Used by the explicit
  // steal-claim so "Claim terminal" routes through the session participant. Built only for
  // `connected-livekit` runtimes; `null` otherwise (the daemon client is the fallback).
  const buildSessionClient = useCallback((): ConnectionClient | null => {
    if (runtime.status !== "connected-livekit") return null;
    const targetIdentity = runtime.livekitServerIdentity;
    if (!targetIdentity || !liveKitFactory) return null;
    const room = roomRef.current ?? (liveKitFactoryIsOverridden ? commonRoom : null);
    if (!room) return null;
    return createClient(ConnectionService, liveKitFactory(room, targetIdentity));
  }, [
    runtime.status,
    runtime.livekitServerIdentity,
    liveKitFactory,
    liveKitFactoryIsOverridden,
    commonRoom,
  ]);

  // The runtime owns its own control lease. The `Session` reference (sessionId + owning daemon
  // client) is passed to `useTerminalControl`, which converts it into a `ConnectedSession` (lease
  // token in hand) once the auto-claim resolves — `connected` stays `null` until then, gating
  // `sendTerminalInput`. The explicit "Claim terminal" steal-claim routes through
  // `buildSessionClient`.
  const session: Session | null =
    client != null ? { sessionId: runtime.sessionId, client } : null;
  const { controlState, connected, claim: claimControl } = useTerminalControl(
    session,
    sessionToken,
    buildSessionClient,
  );

  // The client that carries this session's terminal RPCs: the daemon client for gRPC sessions, the
  // session-scoped client (session-participant routing) for LiveKit sessions. `sessionRoom` is a
  // dependency so the LiveKit client materialises once the room connects.
  const terminalClient: ConnectionClient | null = useMemo(() => {
    if (runtime.status === "connected-grpc") return client ?? null;
    if (runtime.status === "connected-livekit") return buildSessionClient();
    return null;
    // `sessionRoom` intentionally participates so the LiveKit client rebuilds on room connect.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [runtime.status, client, buildSessionClient, sessionRoom]);

  const { terminals, activeTerminalId, setActive, open, close, dropEnded } = useSessionTerminals({
    sessionId: runtime.sessionId,
    sessionToken,
    client: terminalClient,
    controlToken: connected?.controlToken,
  });

  const handleRoom = useCallback(
    (room: Room) => {
      roomRef.current = room;
      setSessionRoom(room);
      onSessionRoom?.(runtime.sessionId, room);
    },
    [onSessionRoom, runtime.sessionId],
  );

  const paneClass = (terminalId: string) =>
    cn("absolute inset-0 h-full w-full", activeTerminalId === terminalId ? "" : "hidden");

  return (
    <div
      data-testid={`sessions-runtime-terminal-${runtime.sessionId}`}
      className={cn("absolute inset-0 flex h-full w-full flex-col", focused ? "" : "hidden")}
      aria-hidden={!focused}
    >
      <SessionTerminalTabs
        terminals={terminals}
        activeTerminalId={activeTerminalId}
        onSelect={setActive}
        onOpen={open}
        onClose={close}
      />

      <div className="relative min-h-0 flex-1">
        {/* Agent pane — the reserved "main" terminal. LiveKit sessions render the VirtualTui
            terminal; gRPC sessions render the direct terminal stream (terminalId ""). */}
        <div data-testid={`sessions-terminal-pane-${AGENT_TERMINAL_ID}`} className={paneClass(AGENT_TERMINAL_ID)}>
          {runtime.status === "connected-livekit" && tokenClient && runtime.livekitRoom && (
            <SessionLiveKitTerminal
              livekitUrl={runtime.livekitUrl ?? ""}
              livekitRoom={runtime.livekitRoom}
              livekitServerIdentity={runtime.livekitServerIdentity ?? ""}
              identity={runtime.identity ?? ""}
              tokenClient={tokenClient}
              onDisconnect={() => onSessionDisconnect?.(runtime.sessionId)}
              mobileShortcuts={focused && activeTerminalId === AGENT_TERMINAL_ID ? mobileShortcuts : undefined}
              onRoom={handleRoom}
            />
          )}
          {runtime.status === "connected-livekit" && !tokenClient && (
            <div className="h-full w-full p-4 text-xs text-muted-foreground">
              Terminal connected to {runtime.livekitRoom}
            </div>
          )}
          {runtime.status === "connected-grpc" && client && (
            <GrpcSessionTerminal
              sessionId={runtime.sessionId}
              sessionToken={sessionToken}
              client={client}
              connected={connected}
              onDisconnect={() => onSessionDisconnect?.(runtime.sessionId)}
              mobileShortcuts={focused && activeTerminalId === AGENT_TERMINAL_ID ? mobileShortcuts : undefined}
            />
          )}
        </div>

        {/* One mounted pane per bash terminal — kept alive whether focused or backgrounded. A bash
            terminal's output stream ending removes only its own tab (never the session). */}
        {terminals.map((id) => (
          <div key={id} data-testid={`sessions-terminal-pane-${id}`} className={paneClass(id)}>
            {terminalClient && (
              <GrpcSessionTerminal
                sessionId={runtime.sessionId}
                sessionToken={sessionToken}
                client={terminalClient}
                connected={connected}
                terminalId={id}
                onDisconnect={() => dropEnded(id)}
                mobileShortcuts={
                  focused && activeTerminalId === id ? mobileShortcuts : undefined
                }
              />
            )}
          </div>
        ))}

        {focused && (
          // The focused runtime carries the terminal-control mutex overlay and the
          // `sessions-detail-terminal-container` marker (existing acceptance contract). The overlay
          // is only rendered when a control client is available — without one the lease is not being
          // managed, so the terminal stays interactive (no spurious "Claim terminal" CTA).
          // `pointer-events-none` lets clicks reach the terminal below when no overlay is showing;
          // the overlay itself re-enables pointer events.
          <div data-testid="sessions-detail-terminal-container" className="absolute inset-0 pointer-events-none">
            {client && (
              <TerminalControlOverlay
                isController={controlState.isController}
                holderScreenId={controlState.holderScreenId}
                onClaim={claimControl}
              />
            )}
          </div>
        )}
      </div>
    </div>
  );
}
