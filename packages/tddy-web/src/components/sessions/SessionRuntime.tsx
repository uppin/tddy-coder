import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Minimize2 } from "lucide-react";
import type { Client } from "@connectrpc/connect";
import { ConnectionService, type SessionEntry } from "../../gen/connection_pb";
import { GhosttyTerminalSession } from "../GhosttyTerminalSession";
import { GrpcSessionTerminal } from "./GrpcSessionTerminal";
import { useSessionTerminalFeed } from "./useSessionTerminalFeed";
import { SessionTerminalTabs } from "./SessionTerminalTabs";
import { SessionAgentConversationPane } from "./SessionAgentConversationPane";
import type { AgentConversation } from "./agentConversationTabs";
import { AGENT_TERMINAL_ID, useSessionTerminals } from "./useSessionTerminals";
import { useChildSessions } from "./useChildSessions";
import { useSessionAttachment } from "./useSessionAttachment";
import { TerminalControlOverlay } from "./TerminalControlOverlay";
import { SessionConnectionOverlay } from "./SessionConnectionOverlay";
import { useTerminalControl, type Session } from "./useTerminalControl";
import type { ByteDelta, SessionRuntimeState } from "./sessionRuntimeRegistry";
import { useConnectionStatus } from "../../rpc/connections/useConnectionStatus";
import { useHasCapability } from "../../rpc/connections/useHasCapability";
import type { HostConnection } from "../../rpc/connections/types";
import type { ToolShortcutDef } from "../../lib/toolShortcuts";
import {
  exitDocumentFullscreen,
  isTargetInActiveFullscreen,
  requestFullscreenForConnectedTerminal,
} from "../../lib/browserFullscreen";
import { safeTestIdPart } from "../../lib/testId";
import { cn } from "../../lib/utils";

type ConnectionClient = Client<typeof ConnectionService>;

export interface SessionRuntimeProps {
  /** This runtime's attached-session state (connection params + status). */
  runtime: SessionRuntimeState;
  /** True when this runtime is the focused (CSS-visible) one. Drives the control overlay + mobile
   *  shortcut overlay; backgrounded runtimes stay mounted but `display:none`. */
  focused: boolean;
  sessionToken: string;
  /** Owning daemon `ConnectionService` client — used for host-served terminal I/O and as the
   *  fallback for the auto-claim-on-attach. Pass `null`/`undefined` until the daemon is reachable. */
  client?: ConnectionClient | null;
  /** The connection to the session's owning daemon — a spawned child conversation attaches its own
   *  session over it. `null` until a host is reachable, which is when no child can be attached. */
  host?: HostConnection | null;
  /** Shortcut presets — shown as the mobile shortcut overlay on the focused runtime only. */
  mobileShortcuts?: ToolShortcutDef[];
  /** Register this session's Agent-terminal text-insert (for the inspector Files-tab click/tap
   *  route). Fired once the terminal mounts. */
  onSessionRegisterInsert?: (sessionId: string, insertInput: (text: string) => void) => void;
  /** Evict this runtime's terminal (e.g. remote session ended). */
  onSessionDisconnect?: (sessionId: string) => void;
  /** Account this session's terminal I/O bytes (see `GhosttyTerminalSession.onBytes`) so the
   *  screen can fold them into the session's inspector counters. */
  onSessionBytes?: (sessionId: string, delta: ByteDelta) => void;
  /** The drawer's full session list — used to discover this session's spawned child conversations
   *  (`orchestratorSessionId === this session`) and render them as tabs. */
  sessions?: ReadonlyArray<SessionEntry>;
  /** Open conversations with agents attached to this session, one tab and one pane each. Held by the
   *  screen rather than here so switching sessions (which backgrounds this runtime) cannot end one. */
  agentConversations?: readonly AgentConversation[];
  /** The focused conversation's id, or `null` when a terminal or child tab holds the pane. */
  activeAgentConversationId?: string | null;
  /** Focus a conversation, or `null` to hand the pane back to the terminal/child tabs. */
  onSelectAgentConversation?: (conversationId: string | null) => void;
  /** Close a conversation's tab — the owner drops it, which cancels it. */
  onCloseAgentConversation?: (conversationId: string) => void;
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
 * The tab strip's trailing ⛶ control puts the active pane into browser full screen (the pane stack
 * is the Fullscreen API target — see the comment at `paneStackRef`). Full screen is a view mode
 * only: nothing unmounts, so a terminal keeps its stream and its control lease across the
 * transition, and the grid re-fits itself through the terminal's own `ResizeObserver`.
 *
 * Feature: `docs/ft/web/session-terminal-tabs.md`, `docs/ft/web/session-drawer.md#fast-session-change`.
 */
export function SessionRuntime({
  runtime,
  focused,
  sessionToken,
  client,
  host = null,
  mobileShortcuts,
  onSessionRegisterInsert,
  onSessionDisconnect,
  onSessionBytes,
  sessions = [],
  agentConversations = [],
  activeAgentConversationId = null,
  onSelectAgentConversation,
  onCloseAgentConversation,
}: SessionRuntimeProps) {
  // This session's own connection, and what it can carry. One terminal component renders either
  // way; what the capabilities decide is where its bytes come from — a session whose wire carries
  // tracks opens its terminal on the connection, one that carries only calls renders the direct
  // stream, which additionally accounts un-acknowledged input.
  const connection = runtime.connection ?? null;
  const carriesMedia = useHasCapability(connection, "media");

  // The connection's own status, sampled as it changes. It drives the handshake overlay for **every**
  // wire: the overlay used to be gated on `connected-livekit`, so a session its host served itself
  // — the configuration that works — showed no connection state at all.
  const connectionStatus = useConnectionStatus(connection);

  // The session-scoped `ConnectionService` client, used by the explicit steal-claim so "Claim
  // terminal" routes to the session's own process rather than to the daemon. The connection
  // memoises it per service, so an unchanged route yields one stable client identity: this callback
  // is invoked inline while rendering, and consumers key stream effects on the client.
  const buildSessionClient = useCallback(
    (): ConnectionClient | null => connection?.clientFor(ConnectionService) ?? null,
    [connection],
  );

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

  // The client that carries this session's terminal RPCs. One expression for every wire: the
  // connection routes to the session's own process where it has one, and to the host that serves it
  // where it does not — which is the daemon client, exactly what the gRPC branch used to reach for
  // by hand.
  const terminalClient: ConnectionClient | null = useMemo(
    () => buildSessionClient(),
    [buildSessionClient],
  );

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
      onSelectAgentConversation?.(null);
      setActive(id);
    },
    [setActive, onSelectAgentConversation],
  );

  const selectChild = useCallback(
    (sessionId: string) => {
      onSelectAgentConversation?.(null);
      setActiveChildSessionId(sessionId);
      setAttachedChildIds((prev) => (prev.includes(sessionId) ? prev : [...prev, sessionId]));
    },
    [onSelectAgentConversation],
  );

  const dropChild = useCallback((sessionId: string) => {
    setAttachedChildIds((prev) => prev.filter((id) => id !== sessionId));
    setActiveChildSessionId((prev) => (prev === sessionId ? null : prev));
  }, []);

  // Expose this session's Agent-terminal text-insert to the screen, keyed by session id, so the
  // inspector Files-tab click/tap route can reach the focused session's terminal.
  const registerInsertInput = useCallback(
    (insertInput: (text: string) => void) => {
      onSessionRegisterInsert?.(runtime.sessionId, insertInput);
    },
    [onSessionRegisterInsert, runtime.sessionId],
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
  // The Agent pane, measured when its terminal opens so the daemon resizes the PTY before replaying.
  const agentPaneRef = useRef<HTMLDivElement>(null);

  // The Agent terminal of a session whose wire carries it, opened on the session's own connection.
  // A host-served session renders `GrpcSessionTerminal` instead, which builds its stream itself so
  // it can also account the daemon's input acks — there is nowhere on a `TerminalFrame` to carry
  // one, so a feed cannot express them (see the changeset).
  const agentTerminalFeed = useSessionTerminalFeed({
    connection: carriesMedia ? connection : null,
    sessionToken,
    controlToken: () => connected?.controlToken ?? "",
    containerRef: agentPaneRef,
  });

  // When this runtime becomes focused with the Agent pane active, return keyboard focus to its
  // terminal — so selection alone makes the session ready to type, no click required. Never steals
  // focus for a backgrounded runtime, and stays out of the way when a bash tab or child pane is up.
  // TODO: `GrpcSessionTerminal` doesn't yet plumb a focus handle through to the terminal, so
  // focus-on-select only works for a session carried over its own room.
  const agentPaneActive =
    activeChildSessionId === null &&
    activeAgentConversationId === null &&
    activeTerminalId === AGENT_TERMINAL_ID;
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

  // Full screen — the pane stack (not an individual pane) is the Fullscreen API target. Only one
  // pane is ever visible, so fullscreening the stack shows exactly the active pane, and it keeps the
  // terminal-control mutex and connection overlays — both siblings of the panes — on screen. Taking
  // a single pane instead would drop the "Claim terminal" CTA behind the fullscreen layer and leave
  // a session whose control another screen holds looking interactive while swallowing every key.
  // The tab strip is deliberately left behind: full screen is the whole viewport for one terminal.
  const paneStackRef = useRef<HTMLDivElement>(null);
  // `active` is containment-based (this stack IS or CONTAINS the fullscreen element) so a parent
  // runtime whose nested child conversation went fullscreen still offers the exit. `owned` is exact,
  // and gates the floating exit control — otherwise a fullscreen child pane would draw its own exit
  // button and its parent's on top of each other.
  const [fullscreenActive, setFullscreenActive] = useState(false);
  const [fullscreenOwned, setFullscreenOwned] = useState(false);

  useEffect(() => {
    const sync = () => {
      const target = paneStackRef.current;
      setFullscreenActive(isTargetInActiveFullscreen(target));
      setFullscreenOwned(target !== null && document.fullscreenElement === target);
    };
    sync();
    document.addEventListener("fullscreenchange", sync);
    document.addEventListener("webkitfullscreenchange", sync as EventListener);
    return () => {
      document.removeEventListener("fullscreenchange", sync);
      document.removeEventListener("webkitfullscreenchange", sync as EventListener);
    };
  }, []);

  // A fullscreen transition re-lays-out the terminal and can drop keyboard focus on the way in or
  // out; the focus-on-select effect above is keyed on selection, so it never fires here. Replay it
  // so a terminal that fills the screen is immediately typeable.
  useEffect(() => {
    if (focused && agentPaneActive) focusAgentTerminalRef.current?.();
  }, [fullscreenActive, focused, agentPaneActive]);

  // Selecting another session (or another tab, for a child runtime) hides this runtime behind
  // `display:none`. Browsers generally drop out of fullscreen when an ancestor is hidden, but not
  // uniformly — and a top-layer element left over a hidden ancestor is a black screen the operator
  // cannot navigate out of. Exit deliberately instead of relying on that, but only from the runtime
  // that actually owns fullscreen: `exitDocumentFullscreen` is document-global.
  useEffect(() => {
    if (!focused && fullscreenOwned) void exitDocumentFullscreen().catch(() => undefined);
  }, [focused, fullscreenOwned]);

  const toggleFullscreen = useCallback(() => {
    const target = paneStackRef.current;
    if (isTargetInActiveFullscreen(target)) {
      void exitDocumentFullscreen().catch(() => undefined);
      return;
    }
    // The request rejects when the browser refuses (an iframe without `allowfullscreen`, a gesture
    // the UA does not count as user-activated). Nothing to recover — the pane stays inline.
    void requestFullscreenForConnectedTerminal(target).catch(() => undefined);
  }, []);

  // A terminal pane (Agent/bash) is visible only when its tab is active AND nothing else holds the
  // pane — a selected child or agent conversation overlays the terminal stack.
  const paneClass = (terminalId: string) =>
    cn(
      "absolute inset-0 h-full w-full",
      activeChildSessionId === null &&
        activeAgentConversationId === null &&
        activeTerminalId === terminalId
        ? ""
        : "hidden",
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
        agentConversations={agentConversations}
        activeAgentConversationId={activeAgentConversationId}
        onSelectAgentConversation={onSelectAgentConversation}
        onCloseAgentConversation={onCloseAgentConversation}
        fullscreenActive={fullscreenActive}
        onToggleFullscreen={toggleFullscreen}
      />

      {/* The pane stack. Its `position` is stated inline as well as in the utility class because
          every pane below is positioned against it, and the terminal panes carry their own inline
          stacking (`terminal-live-pane` takes z-index 2) — a containing block that resolved
          anywhere else would let a pane cover the tab strip that switches it.
          It is also the Fullscreen API target (see `paneStackRef`), which is why it paints its own
          background: a transparent fullscreen element shows the UA's black backdrop through it. */}
      <div
        ref={paneStackRef}
        data-testid={`sessions-terminal-pane-stack-${runtime.sessionId}`}
        className="relative min-h-0 flex-1 bg-background"
        style={{ position: "relative" }}
      >
        {/* Agent pane — the reserved "main" terminal. A session whose connection carries tracks
            reads its bytes off that connection; one that carries only calls renders the direct
            terminal stream (terminalId ""). Both render the same terminal component. */}
        <div
          ref={agentPaneRef}
          data-testid={`sessions-terminal-pane-${AGENT_TERMINAL_ID}`}
          className={paneClass(AGENT_TERMINAL_ID)}
        >
          {carriesMedia && agentTerminalFeed && (
            <GhosttyTerminalSession
              feed={agentTerminalFeed}
              sessionToken={sessionToken}
              sessionId={runtime.sessionId}
              connectionChromePlacement="none"
              onRemoteSessionEnded={() => onSessionDisconnect?.(runtime.sessionId)}
              mobileShortcuts={focused && activeTerminalId === AGENT_TERMINAL_ID ? mobileShortcuts : undefined}
              onRegisterFocus={registerAgentFocus}
              onRegisterInsertInput={registerInsertInput}
              onBytes={handleBytes}
            />
          )}
          {connection && !carriesMedia && (
            <GrpcSessionTerminal
              sessionId={runtime.sessionId}
              sessionToken={sessionToken}
              client={terminalClient}
              connected={connected}
              onDisconnect={() => onSessionDisconnect?.(runtime.sessionId)}
              mobileShortcuts={focused && activeTerminalId === AGENT_TERMINAL_ID ? mobileShortcuts : undefined}
              onRegisterInsertInput={registerInsertInput}
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
              activeAgentConversationId === null && activeChildSessionId === childId
                ? ""
                : "hidden",
            )}
          >
            <SessionChildRuntime
              sessionId={childId}
              focused={focused && activeChildSessionId === childId}
              sessionToken={sessionToken}
              client={client}
              mobileShortcuts={mobileShortcuts}
              onSessionRegisterInsert={onSessionRegisterInsert}
              onSessionBytes={onSessionBytes}
              onDisconnect={dropChild}
              host={host}
              sessions={sessions}
            />
          </div>
        ))}

        {/* One mounted body per open conversation with an attached agent. Kept mounted while
            another tab is up, so switching tabs never re-opens a conversation; only closing the tab
            ends one. */}
        {agentConversations.map((conversation) => (
          <div
            key={conversation.conversationId}
            data-testid={`sessions-agent-pane-${safeTestIdPart(conversation.conversationId)}`}
            className="bg-background"
            // Stated inline, not as utility classes, because the terminal this overlays states its
            // own stacking inline too (`terminal-live-pane` takes z-index 2 and paints over anything
            // that only claims document order), and a conversation that ends up under a terminal
            // still painting is one the operator cannot type into.
            style={{
              position: "absolute",
              inset: 0,
              zIndex: 3,
              display:
                activeAgentConversationId === conversation.conversationId ? "block" : "none",
            }}
          >
            <SessionAgentConversationPane
              sessionId={runtime.sessionId}
              sessionToken={sessionToken}
              daemonInstanceId={conversation.daemonInstanceId}
              agentId={conversation.agentId}
              conversationId={conversation.conversationId}
              client={client ?? undefined}
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
          //
          // Its position and stacking are stated inline for the same reason the conversation panes
          // state theirs: `terminal-live-pane` claims `position: absolute; z-index: 2` inline, so a
          // mutex CTA that only claimed them through utility classes would be painted over by the
          // very terminal canvas it is meant to cover — leaving a session another screen controls
          // looking interactive while swallowing every key.
          <div
            data-testid="sessions-detail-terminal-container"
            className="absolute inset-0 pointer-events-none"
            style={{ position: "absolute", inset: 0, zIndex: 4 }}
          >
            {client && (
              <TerminalControlOverlay
                isController={controlState.isController}
                holderScreenId={controlState.holderScreenId}
                onClaim={claimControl}
              />
            )}
          </div>
        )}

        {/* Connection overlay — covers the panes while the session's connection is still coming up,
            and surfaces a failure if it errors. Renders nothing once connected, so the panes become
            interactive. Driven by the connection itself, so every wire gets a real status: it used
            to be gated on the LiveKit path, leaving a session its host serves directly — a working
            configuration — with no connection state shown at all. */}
        {connection && <SessionConnectionOverlay status={connectionStatus.status} />}

        {/* The tab strip — and with it the strip's own toggle — is outside the fullscreen element,
            so full screen needs its own way back. Rendered only by the stack that actually holds
            fullscreen, and only while it does. Esc still works; this is the in-app equivalent. */}
        {fullscreenOwned && (
          <button
            type="button"
            data-testid="sessions-terminal-fullscreen-exit"
            aria-label="Exit full screen"
            title="Exit full screen"
            onClick={toggleFullscreen}
            className="absolute right-2 top-2 z-20 rounded border border-border bg-background/80 p-1.5 text-muted-foreground opacity-60 transition-opacity hover:opacity-100 hover:text-foreground"
          >
            <Minimize2 className="h-3.5 w-3.5" />
          </button>
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
  host = null,
  mobileShortcuts,
  onSessionRegisterInsert,
  onSessionBytes,
  onDisconnect,
  sessions = [],
}: SessionChildRuntimeProps) {
  const { state: attachment, hint, connectSession } = useSessionAttachment();

  useEffect(() => {
    if (!host) return;
    void connectSession(sessionId, sessionToken, host).catch(() => undefined);
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

  if (!runtime) return null;

  return (
    <SessionRuntime
      runtime={runtime}
      focused={focused}
      sessionToken={sessionToken}
      client={client}
      host={host}
      mobileShortcuts={mobileShortcuts}
      onSessionRegisterInsert={onSessionRegisterInsert}
      onSessionBytes={onSessionBytes}
      onSessionDisconnect={onDisconnect}
      sessions={sessions}
    />
  );
}
