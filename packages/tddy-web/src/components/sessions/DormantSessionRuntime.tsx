import React, { useCallback, useEffect, useRef } from "react";
import type { SessionEntry } from "../../gen/session_pb";
import type { TauriHostWindow } from "../../rpc/daemonTransportFlavour";
import { useSelectedDaemon } from "../../rpc/selectedDaemon";
import { sessionReadsLocalPaths } from "../../lib/sessionReadsLocalPaths";
import type { ToolShortcutDef } from "../../lib/toolShortcuts";
import { cn } from "../../lib/utils";
import { InlineError } from "../ui/inline-error";
import type { AgentConversation } from "./agentConversationTabs";
import { GrpcSessionTerminal } from "./GrpcSessionTerminal";
import { useConversationHostAttachment } from "./SessionAgentConversationLayer";
import { PaneFullscreenExit, TerminalControlLayer } from "./SessionPaneChrome";
import { SessionTerminalTabs } from "./SessionTerminalTabs";
import type { TerminalClient } from "./sessionRuntimeProps";
import { AGENT_TERMINAL_ID, useSessionTerminals } from "./useSessionTerminals";
import { useSessionPaneFullscreen } from "./useSessionPaneFullscreen";
import { useTerminalControl, type Session } from "./useTerminalControl";

/**
 * The tab strip of a session whose agent is not running.
 *
 * A live session's strip lives in `SessionRuntime`, which needs a connection to its agent. A stopped
 * session has none, yet its worktree is still there: "+" opens bash and harness terminals through the
 * owning daemon's `TerminalSessionService` (which starts them in the worktree the session's record
 * names), the terminals the daemon already runs for it are listed, and added agents' conversations
 * get their tabs. What the Agent tab shows is the caller's — `agentTab` — so the stopped Agent tab's
 * body can change without touching the strip.
 *
 * It carries the live strip's features for the terminals it hosts: full screen, the mobile shortcut
 * drawer, the Files-tab insert (into the focused bash tab — the Agent tab of a stopped session takes
 * no input) and the "Claim terminal" toast with its grey-out. It does **not** render the agent
 * conversations' bodies: those outlive any strip (see `SessionAgentConversationLayer`), and this
 * strip only lists their tabs and adopts the element they live in.
 *
 * Feature: docs/ft/web/session-terminal-tabs.md § Stopped sessions
 */
export interface DormantSessionRuntimeProps {
  /** The stopped session. */
  session: SessionEntry;
  sessionToken: string;
  /** Owning daemon `TerminalSessionService` client — the terminals' only wire. */
  terminalClient?: TerminalClient | null;
  /** The body of the Agent tab. */
  agentTab: React.ReactNode;
  /** Shortcut presets for the session's tool, shown as the mobile overlay on the focused bash tab. */
  mobileShortcuts?: ToolShortcutDef[];
  /** Hand the screen the function that types text into the focused bash tab (the Files tab's Insert
   *  route), or `null` when this strip goes away. */
  onRegisterInsert?: (insert: ((text: string) => void) | null) => void;
  /** Open conversations with agents attached to this session — one tab each. */
  agentConversations?: readonly AgentConversation[];
  /** The focused conversation's id, or `null` when a terminal tab holds the pane. */
  activeAgentConversationId?: string | null;
  /** Focus a conversation, or `null` to hand the pane back to the terminal tabs. */
  onSelectAgentConversation?: (conversationId: string | null) => void;
  /** Close a conversation's tab — the owner drops it, which cancels it. */
  onCloseAgentConversation?: (conversationId: string) => void;
  /** The element this session's conversation bodies live in; adopted into the pane stack. */
  conversationHost?: HTMLElement;
}

export function DormantSessionRuntime({
  session,
  sessionToken,
  terminalClient = null,
  agentTab,
  mobileShortcuts,
  onRegisterInsert,
  agentConversations = [],
  activeAgentConversationId = null,
  onSelectAgentConversation,
  onCloseAgentConversation,
  conversationHost,
}: DormantSessionRuntimeProps): React.ReactElement {
  // Control of this session's terminals is claimed on attach, as it is for a live session, so a
  // terminal opened here can take typed input once the daemon grants the claim. Without a client
  // there is nothing to claim through, and none is attempted.
  const controlledSession: Session | null =
    terminalClient !== null ? { sessionId: session.sessionId, client: terminalClient } : null;
  const {
    controlState,
    connected,
    claim: claimControl,
    claimError,
  } = useTerminalControl(controlledSession, sessionToken);

  const { terminals, activeTerminalId, setActive, open, close, dropEnded, error } =
    useSessionTerminals({
      sessionId: session.sessionId,
      sessionToken,
      client: terminalClient,
      controlToken: connected?.controlToken,
    });

  // Whether a file dropped onto the terminals is handed over by its real path or uploaded, decided
  // the same way the live runtime decides it.
  const { servingInstanceId } = useSelectedDaemon();
  const readsLocalPaths = sessionReadsLocalPaths(
    window as TauriHostWindow,
    session,
    servingInstanceId,
  );

  const { paneStackRef, fullscreenActive, fullscreenOwned, toggleFullscreen } =
    useSessionPaneFullscreen(true);
  useConversationHostAttachment(paneStackRef, conversationHost, true);

  // A focused conversation holds the pane; otherwise the active terminal tab does. Only one pane is
  // ever shown, and the terminal panes stay mounted while hidden so their streams keep running.
  const conversationFocused = activeAgentConversationId !== null;
  const terminalPaneShown = (terminalId: string) =>
    !conversationFocused && activeTerminalId === terminalId;
  // The tab a keystroke would reach: a bash tab with nothing covering it. The Agent tab of a stopped
  // session is a transcript and takes no input, so it neither shows the claim toast nor takes inserts.
  const focusedTerminalId =
    !conversationFocused && activeTerminalId !== AGENT_TERMINAL_ID ? activeTerminalId : null;
  const controlledElsewhere =
    focusedTerminalId !== null && terminalClient !== null && !controlState.isController;
  const paneClass = (terminalId: string) =>
    cn(
      "absolute inset-0 h-full w-full",
      terminalPaneShown(terminalId) ? "flex flex-col" : "hidden",
      controlledElsewhere && terminalId === focusedTerminalId && "opacity-60 grayscale",
    );

  // Each bash pane registers its text-insert; the one registered for the focused tab is what the
  // Files tab's Insert reaches.
  const insertByTerminal = useRef(new Map<string, (text: string) => void>());
  const focusedTerminalIdRef = useRef(focusedTerminalId);
  focusedTerminalIdRef.current = focusedTerminalId;
  useEffect(() => {
    onRegisterInsert?.((text) => {
      const id = focusedTerminalIdRef.current;
      if (id !== null) insertByTerminal.current.get(id)?.(text);
    });
    return () => onRegisterInsert?.(null);
  }, [onRegisterInsert]);

  return (
    <div
      data-testid={`sessions-dormant-runtime-${session.sessionId}`}
      className="flex min-h-0 flex-1 flex-col"
    >
      <SessionTerminalTabs
        terminals={terminals}
        activeTerminalId={activeTerminalId}
        onSelect={(terminalId) => {
          onSelectAgentConversation?.(null);
          setActive(terminalId);
        }}
        onOpen={(kind) => {
          // A new terminal is shown at once, so it takes the pane from a focused conversation.
          onSelectAgentConversation?.(null);
          open(kind);
        }}
        onClose={close}
        sessionType={session.sessionType}
        agentConversations={agentConversations}
        activeAgentConversationId={activeAgentConversationId}
        onSelectAgentConversation={onSelectAgentConversation}
        onCloseAgentConversation={onCloseAgentConversation}
        fullscreenActive={fullscreenActive}
        onToggleFullscreen={toggleFullscreen}
      />
      <InlineError
        testId="sessions-terminal-error"
        message={error}
        className="flex-shrink-0 border-b border-border px-3 py-1 text-xs"
      />

      <div
        ref={paneStackRef}
        data-testid={`sessions-terminal-pane-stack-${session.sessionId}`}
        className="relative min-h-0 flex-1 bg-background"
        style={{ position: "relative" }}
      >
        <div
          data-testid={`sessions-terminal-pane-${AGENT_TERMINAL_ID}`}
          className={paneClass(AGENT_TERMINAL_ID)}
        >
          {agentTab}
        </div>

        {terminals.map(({ terminalId }) => (
          <div
            key={terminalId}
            data-testid={`sessions-terminal-pane-${terminalId}`}
            className={paneClass(terminalId)}
          >
            {terminalClient && (
              <DormantTerminalPane
                sessionId={session.sessionId}
                sessionToken={sessionToken}
                client={terminalClient}
                connected={connected}
                terminalId={terminalId}
                readsLocalPaths={readsLocalPaths}
                onEnded={dropEnded}
                mobileShortcuts={terminalId === focusedTerminalId ? mobileShortcuts : undefined}
                insertByTerminal={insertByTerminal.current}
              />
            )}
          </div>
        ))}

        {/* No terminal-control marker here, unlike the live strip: the container exists only to float
            the claim toast, and only over a bash tab (see `focusedTerminalId`), so nothing is
            rendered over the transcript. */}
        {controlledElsewhere && (
          <TerminalControlLayer
            testId={`sessions-dormant-terminal-control-${session.sessionId}`}
            overlay={{
              isController: controlState.isController,
              holderScreenId: controlState.holderScreenId,
              onClaim: claimControl,
              claimError,
            }}
          />
        )}

        {fullscreenOwned && <PaneFullscreenExit onExit={toggleFullscreen} />}
      </div>
    </div>
  );
}

/** One bash tab's terminal, registering its text-insert so the strip can route inserts to it. */
function DormantTerminalPane({
  sessionId,
  sessionToken,
  client,
  connected,
  terminalId,
  readsLocalPaths,
  onEnded,
  mobileShortcuts,
  insertByTerminal,
}: {
  sessionId: string;
  sessionToken: string;
  client: TerminalClient;
  connected: React.ComponentProps<typeof GrpcSessionTerminal>["connected"];
  terminalId: string;
  readsLocalPaths: boolean;
  onEnded: (terminalId: string) => void;
  mobileShortcuts?: ToolShortcutDef[];
  insertByTerminal: Map<string, (text: string) => void>;
}): React.ReactElement {
  const register = useCallback(
    (insert: (text: string) => void) => insertByTerminal.set(terminalId, insert),
    [insertByTerminal, terminalId],
  );
  useEffect(() => () => void insertByTerminal.delete(terminalId), [insertByTerminal, terminalId]);
  return (
    <GrpcSessionTerminal
      sessionId={sessionId}
      sessionToken={sessionToken}
      client={client}
      connected={connected}
      terminalId={terminalId}
      readsLocalPaths={readsLocalPaths}
      onDisconnect={() => onEnded(terminalId)}
      mobileShortcuts={mobileShortcuts}
      onRegisterInsertInput={register}
    />
  );
}
