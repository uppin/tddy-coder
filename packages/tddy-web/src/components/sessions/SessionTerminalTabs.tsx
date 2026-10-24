import React from "react";
import { Maximize2, Minimize2, X } from "lucide-react";
import { AGENT_TERMINAL_ID, type SessionTerminal } from "./useSessionTerminals";
import { terminalKindLabel } from "./cliHarnessOptions";
import { NewTerminalMenu } from "./NewTerminalMenu";
import type { ChildSession } from "./useChildSessions";
import { agentConversationLabel, type AgentConversation } from "./agentConversationTabs";
import { safeTestIdPart } from "../../lib/testId";
import { cn } from "../../lib/utils";

interface SessionTerminalTabsProps {
  /** Open terminals, in tab order (the Agent tab is fixed and rendered first). */
  terminals: readonly SessionTerminal[];
  /** The focused terminal id — `AGENT_TERMINAL_ID` or one of `terminals`. */
  activeTerminalId: string;
  /** Focus a tab. */
  onSelect: (terminalId: string) => void;
  /** Open a terminal of the given kind: `bash`, or a CLI harness's session type. */
  onOpen: (kind: string) => void;
  /** Close a terminal (the ✕ control on its tab). */
  onClose: (terminalId: string) => void;
  /**
   * The session's type. A tool session offers bash only; every other session offers each CLI
   * harness beside bash. Left unset while the session is not yet listed, so the harnesses are
   * offered: the daemon refuses a kind it does not run, and that refusal is shown.
   */
  sessionType?: string;
  /** Spawned child conversations of this session, rendered as tabs after the terminal tabs. */
  childSessions?: readonly ChildSession[];
  /** The selected child conversation's session id, or `null` when a terminal tab is active. */
  activeChildSessionId?: string | null;
  /** Select a child conversation tab. */
  onSelectChild?: (sessionId: string) => void;
  /** Open conversations with agents attached to this session, rendered after the child tabs. */
  agentConversations?: readonly AgentConversation[];
  /** The focused conversation's id, or `null` when a terminal or child tab is active. */
  activeAgentConversationId?: string | null;
  /** Select an agent conversation tab. */
  onSelectAgentConversation?: (conversationId: string) => void;
  /** Close an agent conversation tab — which cancels the conversation it holds. */
  onCloseAgentConversation?: (conversationId: string) => void;
  /** True while the active pane holds browser full screen — flips the control to "Exit full screen". */
  fullscreenActive?: boolean;
  /** Toggle browser full screen for the active pane (the trailing ⛶ control). Omitted ⇒ no control. */
  onToggleFullscreen?: () => void;
}

const TAB_CLASSES =
  "px-3 py-1.5 text-xs font-medium border-b-2 transition-colors whitespace-nowrap";

function tabColorClasses(selected: boolean): string {
  return selected
    ? "border-foreground text-foreground"
    : "border-transparent text-muted-foreground hover:text-foreground";
}

/**
 * Each terminal with the label its tab shows: its kind's name and its ordinal among the tabs of that
 * kind, counted in tab order — "Bash 1", "Warp 1", "Bash 2".
 */
function labelledTerminals(
  terminals: readonly SessionTerminal[],
): { terminal: SessionTerminal; label: string }[] {
  const ordinals = new Map<string, number>();
  return terminals.map((terminal) => {
    const ordinal = (ordinals.get(terminal.kind) ?? 0) + 1;
    ordinals.set(terminal.kind, ordinal);
    return { terminal, label: `${terminalKindLabel(terminal.kind)} ${ordinal}` };
  });
}

/** Display label for a spawned child-conversation tab (its recipe, e.g. "grill me"). */
function childLabel(child: ChildSession): string {
  return (child.recipe || "conversation").replace(/-/g, " ");
}

/**
 * The terminal tab strip at the top of a session's runtime area: a fixed, non-closable Agent tab
 * (the reserved `main` terminal), one tab per open terminal, one per spawned child conversation, one
 * per open conversation with an attached agent, and — pinned to the right edge — the full-screen
 * toggle for whichever pane is currently active. A "+" control sits beside the tabs and opens a menu
 * of bash and every CLI harness the session can run. Styled after `InspectorTabs`.
 *
 * The tabs scroll horizontally when they overflow; the "+" and the full-screen control sit *outside*
 * that scroller, so a session with a dozen terminals and conversations cannot push them out of reach,
 * and the menu is not clipped by it.
 */
export function SessionTerminalTabs({
  terminals,
  activeTerminalId,
  onSelect,
  onOpen,
  onClose,
  sessionType,
  childSessions = [],
  activeChildSessionId = null,
  onSelectChild,
  agentConversations = [],
  activeAgentConversationId = null,
  onSelectAgentConversation,
  onCloseAgentConversation,
  fullscreenActive = false,
  onToggleFullscreen,
}: SessionTerminalTabsProps) {
  // A terminal tab (Agent or bash) is only selected when nothing else holds the pane — a child
  // conversation or a conversation with an attached agent. Two selected tabs would claim two panes
  // are showing at once.
  const childActive = activeChildSessionId !== null;
  const conversationActive = activeAgentConversationId !== null;
  const terminalHoldsPane = !childActive && !conversationActive;
  const agentSelected = terminalHoldsPane && activeTerminalId === AGENT_TERMINAL_ID;

  return (
    <div
      data-testid="sessions-terminal-tabs"
      className="flex items-center border-b border-border flex-shrink-0"
    >
      <div className="flex min-w-0 flex-1 items-center overflow-x-auto">
        <button
          type="button"
          data-testid="sessions-terminal-tab-agent"
          aria-selected={agentSelected}
          onClick={() => onSelect(AGENT_TERMINAL_ID)}
          className={cn(TAB_CLASSES, tabColorClasses(agentSelected))}
        >
          Agent
        </button>

        {labelledTerminals(terminals).map(({ terminal: { terminalId: id }, label }) => {
          const selected = terminalHoldsPane && activeTerminalId === id;
          return (
            <div key={id} className="flex items-center">
              <button
                type="button"
                data-testid={`sessions-terminal-tab-${id}`}
                aria-selected={selected}
                onClick={() => onSelect(id)}
                className={cn(TAB_CLASSES, tabColorClasses(selected), "pr-1")}
              >
                {label}
              </button>
              <button
                type="button"
                data-testid={`sessions-terminal-tab-close-${id}`}
                aria-label={`Close ${label}`}
                onClick={() => onClose(id)}
                className="mr-1 rounded p-0.5 text-muted-foreground hover:bg-muted hover:text-foreground"
              >
                <X className="h-3 w-3" />
              </button>
            </div>
          );
        })}

        {childSessions.map((child) => {
          const selected = !conversationActive && activeChildSessionId === child.sessionId;
          return (
            <button
              key={child.sessionId}
              type="button"
              role="tab"
              data-testid={`sessions-child-tab-${child.sessionId}`}
              aria-selected={selected}
              onClick={() => onSelectChild?.(child.sessionId)}
              className={cn(TAB_CLASSES, tabColorClasses(selected))}
            >
              {childLabel(child)}
            </button>
          );
        })}

        {/* One tab per open conversation with an attached agent. Keyed by the *conversation* id
            rather than the agent id: an agent can be attached with no conversation open, and closing
            a tab cancels a conversation, not an attachment. */}
        {agentConversations.map((conversation) => {
          const tabTestId = `sessions-agent-tab-${safeTestIdPart(conversation.conversationId)}`;
          const selected = activeAgentConversationId === conversation.conversationId;
          return (
            <div key={conversation.conversationId} className="flex items-center">
              <button
                type="button"
                role="tab"
                data-testid={tabTestId}
                aria-selected={selected}
                onClick={() => onSelectAgentConversation?.(conversation.conversationId)}
                className={cn(TAB_CLASSES, tabColorClasses(selected), "pr-1")}
              >
                {agentConversationLabel(conversation)}
              </button>
              {/* A sibling of the tab, never a child of it: nesting it would make closing a
                  conversation also a request to look at it, and would put a ✕ inside the tab's
                  accessible name. */}
              <button
                type="button"
                data-testid={`${tabTestId}-close`}
                aria-label={`Close conversation with ${agentConversationLabel(conversation)}`}
                onClick={() => onCloseAgentConversation?.(conversation.conversationId)}
                className="mr-1 rounded p-0.5 text-muted-foreground hover:bg-muted hover:text-foreground"
              >
                <X className="h-3 w-3" />
              </button>
            </div>
          );
        })}
      </div>

      <NewTerminalMenu sessionType={sessionType} onOpen={onOpen} />

      {/* Full screen — acts on whichever pane is active, since only one is ever visible. */}
      {onToggleFullscreen && (
        <button
          type="button"
          data-testid="sessions-terminal-fullscreen"
          aria-label={fullscreenActive ? "Exit full screen" : "Enter full screen"}
          aria-pressed={fullscreenActive}
          title={fullscreenActive ? "Exit full screen" : "Enter full screen"}
          onClick={onToggleFullscreen}
          className="flex-shrink-0 px-2 py-1.5 text-muted-foreground hover:text-foreground"
        >
          {fullscreenActive ? (
            <Minimize2 className="h-3.5 w-3.5" />
          ) : (
            <Maximize2 className="h-3.5 w-3.5" />
          )}
        </button>
      )}
    </div>
  );
}
