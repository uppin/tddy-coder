import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createClient, type Client, type Transport } from "@connectrpc/connect";
import type { Room } from "livekit-client";
import { ConnectionService, type SessionEntry } from "../../gen/connection_pb";
import type { TokenService } from "../../gen/token_pb";
import { SessionLiveKitTerminal } from "./SessionLiveKitTerminal";
import { GrpcSessionTerminal } from "./GrpcSessionTerminal";
import { SessionTerminalTabs } from "./SessionTerminalTabs";
import { AGENT_TERMINAL_ID, useSessionTerminals } from "./useSessionTerminals";
import { useChildSessions } from "./useChildSessions";
import { useSessionAttachment } from "./useSessionAttachment";
import { TerminalControlOverlay } from "./TerminalControlOverlay";
import { SessionConnectionOverlay } from "./SessionConnectionOverlay";
import { useTerminalControl, type Session } from "./useTerminalControl";
import type { ByteDelta, SessionRuntimeState } from "./sessionRuntimeRegistry";
import type { ToolShortcutDef } from "../../lib/toolShortcuts";
import type { LiveKitChromeStatus } from "../../lib/liveKitStatusPresentation";
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
  /** Account this session's terminal I/O bytes (see `GhosttyTerminalLiveKit.onBytes`) so the
   *  screen can fold them into the session's inspector counters. */
  onSessionBytes?: (sessionId: string, delta: ByteDelta) => void;
  /** LiveKit transport factory — builds the session-scoped client transport for the explicit
   *  steal-claim (`ClaimTerminalControl`, session-participant routing) and, for `connected-livekit`
   *  sessions, the bash terminals' I/O. */
  liveKitFactory?: (room: Room, targetIdentity: string) => Transport;
  /** True when `liveKitFactory` is a test double that ignores its `room` argument — the common
   *  room is then an acceptable stand-in for the session's own room. */
  liveKitFactoryIsOverridden?: boolean;
  /** Shared common room — used as the session-room stand-in when the factory is overridden. */
  commonRoom?: Room | null;
  /** The drawer's full session list — used to discover this session's spawned child conversations
   *  (`orchestratorSessionId === this session`) and render them as tabs. */
  sessions?: ReadonlyArray<SessionEntry>;
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
  onSessionBytes,
  liveKitFactory,
  liveKitFactoryIsOverridden = false,
  commonRoom = null,
  sessions = [],
}: SessionRuntimeProps) {
  // The runtime's own connected Room, captured via the terminal's `onRoom`. Stored both in a ref
  // (for the lazy steal-claim client) and in state (so the memoized session-scoped terminal client
  // below rebuilds once the room connects).
  const roomRef = useRef<Room | null>(null);
  const [sessionRoom, setSessionRoom] = useState<Room | null>(null);

  // The LiveKit room's connection status, reported by the Agent pane's `SessionLiveKitTerminal`.
  // Drives the connection overlay that covers the panes until the room connects. Starts "connecting"
  // (the room's handshake — token request + join — hasn't reported yet). Only meaningful for
  // `connected-livekit` runtimes; the `connected-grpc` path has no such handshake, so it never shows
  // the overlay.
  const [liveKitStatus, setLiveKitStatus] = useState<LiveKitChromeStatus>("connecting");

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

  // Spawned child conversations of this session (tagged with `orchestratorSessionId = this session`).
  // Each renders as a tab after the bash tabs; selecting one attaches that child and shows its pane.
  const childSessions = useChildSessions(runtime.sessionId, sessions);

  // The selected child conversation, or `null` when a terminal (Agent/bash) tab is active. Children
  // are attached lazily: a child's runtime pane is only mounted (and its `ConnectSession` fired)
  // once its tab has been selected, and it then stays mounted across further tab switches.
  const [activeChildSessionId, setActiveChildSessionId] = useState<string | null>(null);
  const [attachedChildIds, setAttachedChildIds] = useState<string[]>([]);

  const selectTerminal = useCallback(
    (id: string) => {
      setActiveChildSessionId(null);
      setActive(id);
    },
    [setActive],
  );

  const selectChild = useCallback((sessionId: string) => {
    setActiveChildSessionId(sessionId);
    setAttachedChildIds((prev) => (prev.includes(sessionId) ? prev : [...prev, sessionId]));
  }, []);

  const dropChild = useCallback((sessionId: string) => {
    setAttachedChildIds((prev) => prev.filter((id) => id !== sessionId));
    setActiveChildSessionId((prev) => (prev === sessionId ? null : prev));
  }, []);

  const handleRoom = useCallback(
    (room: Room) => {
      roomRef.current = room;
      setSessionRoom(room);
      onSessionRoom?.(runtime.sessionId, room);
    },
    [onSessionRoom, runtime.sessionId],
  );

  // Account the Agent terminal's byte traffic to this session's own id, so the inspector counters
  // tick per output chunk / input yield even while this runtime is backgrounded.
  const handleBytes = useCallback(
    (delta: ByteDelta) => {
      onSessionBytes?.(runtime.sessionId, delta);
    },
    [onSessionBytes, runtime.sessionId],
  );

  // Imperative focus handle for the Agent pane's terminal. Each terminal self-focuses once at
  // mount, so first-selection works on its own; re-selecting an already-mounted runtime only flips
  // CSS visibility, so we replay focus here when this runtime comes to the foreground.
  const focusAgentTerminalRef = useRef<(() => void) | null>(null);
  const registerAgentFocus = useCallback((focus: () => void) => {
    focusAgentTerminalRef.current = focus;
  }, []);
  // The runtime's outer container — used by the focus guard to tell "focus landed inside me" from
  // "a sibling session's terminal stole focus".
  const containerRef = useRef<HTMLDivElement>(null);

  // When this runtime becomes focused with the Agent pane active, return keyboard focus to its
  // terminal — so selection alone makes the session ready to type, no click required. Never steals
  // focus for a backgrounded runtime, and stays out of the way when a bash tab or child pane is up.
  // TODO: gRPC sessions (`connected-grpc`, GrpcSessionTerminal/GhosttyTerminalGrpc) don't yet plumb
  // a focus handle, so focus-on-select is LiveKit-only for now.
  const agentPaneActive = activeChildSessionId === null && activeTerminalId === AGENT_TERMINAL_ID;
  useEffect(() => {
    if (focused && agentPaneActive) {
      focusAgentTerminalRef.current?.();
    }
  }, [focused, agentPaneActive]);

  // Focus guard: a backgrounded session keeps its terminal mounted, and a terminal that opens (or
  // is re-selected) while still transiently visible auto-focuses itself — ghostty-web focuses on
  // open and re-asserts it on a deferred timer. That lets a background session's terminal steal
  // keyboard focus from the foreground one a beat after selection. While this runtime is the
  // foreground one with its Agent pane active, reclaim focus whenever it lands in a *different*
  // session's terminal (never for focus that legitimately moves to the drawer, inspector, etc.).
  useEffect(() => {
    if (!focused || !agentPaneActive) return;
    const self = containerRef.current;
    if (!self) return;
    const onFocusIn = (e: FocusEvent) => {
      const target = e.target as Node | null;
      if (!target || self.contains(target)) return;
      const stealer = (target as HTMLElement).closest?.(
        "[data-testid^='sessions-runtime-terminal-']",
      );
      if (stealer) focusAgentTerminalRef.current?.();
    };
    document.addEventListener("focusin", onFocusIn, true);
    return () => document.removeEventListener("focusin", onFocusIn, true);
  }, [focused, agentPaneActive]);

  // A terminal pane (Agent/bash) is visible only when its tab is active AND no child conversation
  // is selected — a selected child's pane overlays the terminal stack.
  const paneClass = (terminalId: string) =>
    cn(
      "absolute inset-0 h-full w-full",
      activeChildSessionId === null && activeTerminalId === terminalId ? "" : "hidden",
    );

  return (
    <div
      ref={containerRef}
      data-testid={`sessions-runtime-terminal-${runtime.sessionId}`}
      className={cn("absolute inset-0 flex h-full w-full flex-col", focused ? "" : "hidden")}
      aria-hidden={!focused}
    >
      <SessionTerminalTabs
        terminals={terminals}
        activeTerminalId={activeTerminalId}
        onSelect={selectTerminal}
        onOpen={open}
        onClose={close}
        childSessions={childSessions}
        activeChildSessionId={activeChildSessionId}
        onSelectChild={selectChild}
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
              onRegisterFocus={registerAgentFocus}
              onConnectionStatusChange={setLiveKitStatus}
              onBytes={handleBytes}
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

        {/* One mounted pane per attached child conversation — a nested runtime that attaches the
            child session over its own `ConnectSession` and renders the child's terminal. Visible
            only while its tab is active; kept mounted (streaming) once opened. */}
        {attachedChildIds.map((childId) => (
          <div
            key={childId}
            data-testid={`sessions-child-pane-${childId}`}
            className={cn(
              "absolute inset-0 h-full w-full",
              activeChildSessionId === childId ? "" : "hidden",
            )}
          >
            <SessionChildRuntime
              sessionId={childId}
              focused={focused && activeChildSessionId === childId}
              sessionToken={sessionToken}
              client={client}
              tokenClient={tokenClient}
              mobileShortcuts={mobileShortcuts}
              onSessionRoom={onSessionRoom}
              onSessionBytes={onSessionBytes}
              onDisconnect={dropChild}
              liveKitFactory={liveKitFactory}
              liveKitFactoryIsOverridden={liveKitFactoryIsOverridden}
              commonRoom={commonRoom}
              sessions={sessions}
            />
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

        {/* Connection overlay — covers the panes while the session's LiveKit room is still
            handshaking, and surfaces a failure if it errors. Renders nothing once connected, so the
            panes become interactive. LiveKit-only: the `connected-grpc` path has no such handshake. */}
        {runtime.status === "connected-livekit" && (
          <SessionConnectionOverlay status={liveKitStatus} />
        )}
      </div>
    </div>
  );
}

interface SessionChildRuntimeProps {
  /** The spawned child conversation's session id. */
  sessionId: string;
  /** True when this child pane is the visible/interactive one. */
  focused: boolean;
  sessionToken: string;
  client?: ConnectionClient | null;
  tokenClient?: TokenClient;
  mobileShortcuts?: ToolShortcutDef[];
  onSessionRoom?: (sessionId: string, room: Room) => void;
  /** Account this child's terminal I/O bytes to its own session id (see `SessionRuntime.onSessionBytes`). */
  onSessionBytes?: (sessionId: string, delta: ByteDelta) => void;
  /** Drop this child (its output stream ended) — removes the pane and returns focus to the parent. */
  onDisconnect?: (sessionId: string) => void;
  liveKitFactory?: (room: Room, targetIdentity: string) => Transport;
  liveKitFactoryIsOverridden?: boolean;
  commonRoom?: Room | null;
  sessions?: ReadonlyArray<SessionEntry>;
}

/**
 * A spawned child conversation rendered inside its parent's runtime. It owns its own attachment —
 * attaching the child over `ConnectSession` the first time it is mounted (i.e. when its tab is
 * first selected) — and, once connected, renders a nested {@link SessionRuntime} for the child so
 * the child gets its own Agent + bash terminals (and, recursively, its own spawned conversations).
 */
function SessionChildRuntime({
  sessionId,
  focused,
  sessionToken,
  client,
  tokenClient,
  mobileShortcuts,
  onSessionRoom,
  onSessionBytes,
  onDisconnect,
  liveKitFactory,
  liveKitFactoryIsOverridden,
  commonRoom,
  sessions = [],
}: SessionChildRuntimeProps) {
  const { state: attachment, connectSession } = useSessionAttachment();

  useEffect(() => {
    if (!client) return;
    void connectSession(sessionId, sessionToken, client).catch(() => undefined);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [client, sessionId, sessionToken]);

  // Project the attachment into a `SessionRuntimeState` the nested runtime can render. Until the
  // child's `ConnectSession` resolves there is nothing to render yet.
  const runtime = useMemo<SessionRuntimeState | null>(() => {
    if (attachment.status === "connected-livekit") {
      return {
        sessionId,
        attached: true,
        status: "connected-livekit",
        livekitUrl: attachment.livekitUrl,
        livekitRoom: attachment.livekitRoom,
        livekitServerIdentity: attachment.livekitServerIdentity,
        identity: attachment.identity,
        bytesIn: 0,
        bytesOut: 0,
        lastDataReceivedAt: null,
      };
    }
    if (attachment.status === "connected-grpc") {
      return {
        sessionId,
        attached: true,
        status: "connected-grpc",
        livekitUrl: "",
        livekitRoom: "",
        livekitServerIdentity: "",
        identity: "",
        bytesIn: 0,
        bytesOut: 0,
        lastDataReceivedAt: null,
      };
    }
    return null;
  }, [attachment, sessionId]);

  if (!runtime) return null;

  return (
    <SessionRuntime
      runtime={runtime}
      focused={focused}
      sessionToken={sessionToken}
      client={client}
      tokenClient={tokenClient}
      mobileShortcuts={mobileShortcuts}
      onSessionRoom={onSessionRoom}
      onSessionBytes={onSessionBytes}
      onSessionDisconnect={onDisconnect}
      liveKitFactory={liveKitFactory}
      liveKitFactoryIsOverridden={liveKitFactoryIsOverridden}
      commonRoom={commonRoom}
      sessions={sessions}
    />
  );
}
