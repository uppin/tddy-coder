import React from "react";
import { Plus, X } from "lucide-react";
import { AGENT_TERMINAL_ID } from "./useSessionTerminals";
import type { ChildSession } from "./useChildSessions";
import { cn } from "../../lib/utils";

interface SessionTerminalTabsProps {
  /** Open bash terminal ids, in tab order (the Agent tab is fixed and rendered first). */
  terminals: readonly string[];
  /** The focused terminal id — `AGENT_TERMINAL_ID` or one of `terminals`. */
  activeTerminalId: string;
  /** Focus a tab. */
  onSelect: (terminalId: string) => void;
  /** Open a new bash terminal (the "+" control). */
  onOpen: () => void;
  /** Close a bash terminal (the ✕ control on its tab). */
  onClose: (terminalId: string) => void;
  /** Spawned child conversations of this session, rendered as tabs after the bash tabs. */
  childSessions?: readonly ChildSession[];
  /** The selected child conversation's session id, or `null` when a terminal tab is active. */
  activeChildSessionId?: string | null;
  /** Select a child conversation tab. */
  onSelectChild?: (sessionId: string) => void;
}

const TAB_CLASSES =
  "px-3 py-1.5 text-xs font-medium border-b-2 transition-colors whitespace-nowrap";

function tabColorClasses(selected: boolean): string {
  return selected
    ? "border-foreground text-foreground"
    : "border-transparent text-muted-foreground hover:text-foreground";
}

/** Short display label for a bash terminal tab (e.g. `bash-1` → "bash 1"). */
function terminalLabel(terminalId: string): string {
  return terminalId.replace(/-/g, " ");
}

/** Display label for a spawned child-conversation tab (its recipe, e.g. "grill me"). */
function childLabel(child: ChildSession): string {
  return (child.recipe || "conversation").replace(/-/g, " ");
}

/**
 * The terminal tab strip at the top of a session's runtime area: a fixed, non-closable Agent tab
 * (the reserved `main` terminal) followed by one tab per open bash terminal, and a trailing "+"
 * control that opens another bash terminal. Styled after `InspectorTabs`.
 */
export function SessionTerminalTabs({
  terminals,
  activeTerminalId,
  onSelect,
  onOpen,
  onClose,
  childSessions = [],
  activeChildSessionId = null,
  onSelectChild,
}: SessionTerminalTabsProps) {
  // A terminal tab (Agent or bash) is only selected when no child conversation is active.
  const childActive = activeChildSessionId !== null;
  const agentSelected = !childActive && activeTerminalId === AGENT_TERMINAL_ID;

  return (
    <div
      data-testid="sessions-terminal-tabs"
      className="flex items-center border-b border-border flex-shrink-0 overflow-x-auto"
    >
      <button
        type="button"
        data-testid="sessions-terminal-tab-agent"
        aria-selected={agentSelected}
        onClick={() => onSelect(AGENT_TERMINAL_ID)}
        className={cn(TAB_CLASSES, tabColorClasses(agentSelected))}
      >
        Agent
      </button>

      {terminals.map((id) => {
        const selected = !childActive && activeTerminalId === id;
        return (
          <div key={id} className="flex items-center">
            <button
              type="button"
              data-testid={`sessions-terminal-tab-${id}`}
              aria-selected={selected}
              onClick={() => onSelect(id)}
              className={cn(TAB_CLASSES, tabColorClasses(selected), "pr-1")}
            >
              {terminalLabel(id)}
            </button>
            <button
              type="button"
              data-testid={`sessions-terminal-tab-close-${id}`}
              aria-label={`Close ${terminalLabel(id)}`}
              onClick={() => onClose(id)}
              className="mr-1 rounded p-0.5 text-muted-foreground hover:bg-muted hover:text-foreground"
            >
              <X className="h-3 w-3" />
            </button>
          </div>
        );
      })}

      {childSessions.map((child) => {
        const selected = activeChildSessionId === child.sessionId;
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

      <button
        type="button"
        data-testid="sessions-terminal-tab-new"
        aria-label="Open a new terminal"
        onClick={onOpen}
        className="px-2 py-1.5 text-muted-foreground hover:text-foreground"
      >
        <Plus className="h-3.5 w-3.5" />
      </button>
    </div>
  );
}
