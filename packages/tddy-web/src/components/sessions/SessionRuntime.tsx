import React, { useCallback, useMemo, useRef, useState } from "react";
import { Minimize2 } from "lucide-react";
import { TerminalSessionService } from "../../gen/terminal_session_pb";
import { GhosttyTerminalSession } from "../GhosttyTerminalSession";
import { GrpcSessionTerminal } from "./GrpcSessionTerminal";
import { useSessionTerminalFeed } from "./useSessionTerminalFeed";
import { SessionTerminalTabs } from "./SessionTerminalTabs";
import { SessionAgentConversationPane } from "./SessionAgentConversationPane";
import { SessionChildRuntime } from "./SessionChildRuntime";
import { AGENT_TERMINAL_ID, useSessionTerminals } from "./useSessionTerminals";
import { useChildSessions } from "./useChildSessions";
import { TerminalControlOverlay } from "./TerminalControlOverlay";
import { SessionConnectionOverlay } from "./SessionConnectionOverlay";
import { useTerminalControl, type Session } from "./useTerminalControl";
import { useSessionPaneFullscreen } from "./useSessionPaneFullscreen";
import { useAgentTerminalFocus } from "./useAgentTerminalFocus";
import type { ByteDelta } from "./sessionRuntimeRegistry";
import type { SessionRuntimeProps, TerminalClient } from "./sessionRuntimeProps";
import { useConnectionStatus } from "../../rpc/connections/useConnectionStatus";
import { useHasCapability } from "../../rpc/connections/useHasCapability";
import { safeTestIdPart } from "../../lib/testId";
import { cn } from "../../lib/utils";
import { InlineError } from "../ui/inline-error";

export type { SessionRuntimeProps } from "./sessionRuntimeProps";

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
  terminalClient,
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

  // The session-scoped `TerminalSessionService` client, used by the explicit steal-claim so "Claim
  // terminal" routes to the session's own process rather than to the daemon. The connection
  // memoises it per service, so an unchanged route yields one stable client identity: this callback
  // is invoked inline while rendering, and consumers key stream effects on the client.
  const buildSessionTerminalClient = useCallback(
    (): TerminalClient | null => connection?.clientFor(TerminalSessionService) ?? null,
    [connection],
  );

  // The runtime owns its own control lease. The `Session` reference (sessionId + owning daemon
  // terminal client) is passed to `useTerminalControl`, which converts it into a `ConnectedSession`
  // (lease token in hand) once the auto-claim resolves — `connected` stays `null` until then, gating
  // `sendTerminalInput`. The explicit "Claim terminal" steal-claim routes through
  // `buildSessionTerminalClient`.
  const session: Session | null =
    terminalClient != null ? { sessionId: runtime.sessionId, client: terminalClient } : null;
  const { controlState, connected, claim: claimControl, claimError } = useTerminalControl(
    session,
    sessionToken,
    buildSessionTerminalClient,
  );

  // The client that carries this session's terminal RPCs. One expression for every wire: the
  // connection routes to the session's own process where it has one, and to the host that serves it
  // where it does not — which is the daemon client, exactly what the gRPC branch used to reach for
  // by hand.
  const sessionTerminalClient: TerminalClient | null = useMemo(
    () => buildSessionTerminalClient(),
    [buildSessionTerminalClient],
  );

  const {
    terminals,
    activeTerminalId,
    setActive,
    open,
    close,
    dropEnded,
    error: terminalsError,
  } = useSessionTerminals({
    sessionId: runtime.sessionId,
    sessionToken,
    client: sessionTerminalClient,
    controlToken: connected?.controlToken,
  });

  // Spawned child conversations of this session (tagged with `orchestratorSessionId = this session`).
  // Each renders as a tab after the bash tabs; selecting one attaches that child and shows its pane.
  const childSessions = useChildSessions(runtime.sessionId, sessions);

  // This session's type, which decides what its "+" menu offers. A session the drawer's list does
  // not carry yet has no type here, so the tab strip offers the harnesses and the daemon refuses any
  // it does not run.
  const sessionType = sessions.find((s) => s.sessionId === runtime.sessionId)?.sessionType;

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

  // The Agent pane, measured when its terminal opens so the daemon resizes the PTY before replaying.
  const agentPaneRef = useRef<HTMLDivElement>(null);

  // The Agent terminal of a session whose wire carries it, opened on the session's own connection.
  // A host-served session renders `GrpcSessionTerminal` instead, which builds its stream itself so
  // it can also account the daemon's input acks — there is nowhere on a `TerminalFrame` to carry
  // one, so a feed cannot express them (see the changeset).
  // Why the Agent terminal's last send did not reach the PTY, when its feed can say (see
  // `TerminalOptions.onInputDelivery`); the direct stream below keeps its own indicator.
  const [agentInputError, setAgentInputError] = useState<string | null>(null);
  const agentTerminalFeed = useSessionTerminalFeed({
    connection: carriesMedia ? connection : null,
    sessionToken,
    controlToken: () => connected?.controlToken ?? "",
    containerRef: agentPaneRef,
    onInputDelivery: setAgentInputError,
  });

  const agentPaneActive =
    activeChildSessionId === null &&
    activeAgentConversationId === null &&
    activeTerminalId === AGENT_TERMINAL_ID;
  // The pane stack's full screen (see `useSessionPaneFullscreen`), and the Agent terminal's keyboard
  // focus, which a fullscreen transition can drop.
  const { paneStackRef, fullscreenActive, fullscreenOwned, toggleFullscreen } =
    useSessionPaneFullscreen(focused);
  const { containerRef, registerAgentFocus } = useAgentTerminalFocus({
    focused,
    agentPaneActive,
    fullscreenActive,
  });

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
        sessionType={sessionType}
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
      <InlineError
        testId="sessions-terminal-error"
        message={terminalsError}
        className="flex-shrink-0 border-b border-border px-3 py-1 text-xs"
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
          {carriesMedia && (
            <InlineError
              testId="terminal-input-error"
              message={agentInputError && `Input not delivered: ${agentInputError}`}
              className="pointer-events-none absolute bottom-2 left-2 z-10 max-w-[90%] rounded border border-destructive/40 bg-background/90 px-2 py-1 text-xs"
            />
          )}
          {connection && !carriesMedia && (
            <GrpcSessionTerminal
              sessionId={runtime.sessionId}
              sessionToken={sessionToken}
              client={sessionTerminalClient}
              connected={connected}
              onDisconnect={() => onSessionDisconnect?.(runtime.sessionId)}
              mobileShortcuts={focused && activeTerminalId === AGENT_TERMINAL_ID ? mobileShortcuts : undefined}
              onRegisterInsertInput={registerInsertInput}
            />
          )}
        </div>

        {/* One mounted pane per bash terminal — kept alive whether focused or backgrounded. A bash
            terminal's output stream ending removes only its own tab (never the session). */}
        {terminals.map(({ terminalId: id }) => (
          <div key={id} data-testid={`sessions-terminal-pane-${id}`} className={paneClass(id)}>
            {sessionTerminalClient && (
              <GrpcSessionTerminal
                sessionId={runtime.sessionId}
                sessionToken={sessionToken}
                client={sessionTerminalClient}
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
              terminalClient={terminalClient}
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
            // still painting is one the operator cannot type into. Above the terminal-control
            // overlay (3) as well: that overlay is about typing into the *terminal*, and a
            // conversation is not it.
            style={{
              position: "absolute",
              inset: 0,
              zIndex: 4,
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
          // is only rendered when a terminal-control client is available — without one the lease is
          // not being managed, so the terminal stays interactive (no spurious "Claim terminal" CTA).
          // `pointer-events-none` lets clicks reach the terminal below when no overlay is showing;
          // the overlay itself re-enables pointer events.
          //
          // Its position and stacking are stated inline for the same reason the conversation panes
          // state theirs: `terminal-live-pane` claims `position: absolute; z-index: 2` inline, so a
          // mutex CTA that only claimed them through utility classes would be painted over by the
          // very terminal canvas it is meant to cover — leaving a session another screen controls
          // looking interactive while swallowing every key.
          //
          // The ladder in this stack, bottom up: terminal panes (2), this mutex overlay (3), an
          // open agent conversation (4). Each covers what it is about and nothing else.
          <div
            data-testid="sessions-detail-terminal-container"
            className="absolute inset-0 pointer-events-none"
            style={{ position: "absolute", inset: 0, zIndex: 3 }}
          >
            {terminalClient && (
              <TerminalControlOverlay
                isController={controlState.isController}
                holderScreenId={controlState.holderScreenId}
                onClaim={claimControl}
                claimError={claimError}
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
