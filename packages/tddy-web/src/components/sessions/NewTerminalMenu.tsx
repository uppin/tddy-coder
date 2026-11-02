import React, { useEffect, useRef, useState } from "react";
import { Plus } from "lucide-react";
import type { TmuxSessionInfo } from "../../gen/terminal_session_pb";
import {
  BASH_KIND,
  CLI_HARNESS_OPTIONS,
  TMUX_KIND,
  TOOL_SESSION_TYPE,
  terminalKindLabel,
} from "./cliHarnessOptions";
import type { TmuxMenuState } from "./useTmuxSessions";

interface NewTerminalMenuProps {
  /** The session's type: a tool session offers bash only. Unset offers every harness. */
  sessionType?: string;
  /** Open a terminal of the chosen kind: `bash`, a CLI harness's session type, or `tmux` attached
   *  to `tmuxSession`. */
  onOpen: (kind: string, tmuxSession?: string) => void;
  /** The host's tmux sessions, offered as a section of the menu; absent offers no tmux section. The
   *  section lists again each time the menu opens. */
  tmux?: TmuxMenuState;
}

/**
 * The tab strip's "+" control: a menu of bash and every CLI harness this session can run — the same
 * list the Create pane offers — and, below them, the host's tmux sessions. Rendered outside the tab
 * scroller so the menu is not clipped by it.
 *
 * The tmux section is a group of its own, so the bash and harness list stays exactly the terminals
 * it offers, whatever state the tmux listing is in.
 */
export function NewTerminalMenu({ sessionType, onOpen, tmux: tmuxState }: NewTerminalMenuProps) {
  const [menuOpen, setMenuOpen] = useState(false);
  const newTabRef = useRef<HTMLDivElement>(null);
  const newTabButtonRef = useRef<HTMLButtonElement>(null);

  // A tool session's participant runs bash only, so there is no tmux server on its side to list.
  const tmux = sessionType === TOOL_SESSION_TYPE ? undefined : tmuxState;

  // The "+" menu closes on Escape and on a press outside it. Both are listened for only while it is
  // open, so an idle tab strip holds no document listeners.
  useEffect(() => {
    if (!menuOpen) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      setMenuOpen(false);
      newTabButtonRef.current?.focus();
    };
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (target instanceof Node && newTabRef.current?.contains(target)) return;
      setMenuOpen(false);
    };
    document.addEventListener("keydown", onKeyDown);
    document.addEventListener("pointerdown", onPointerDown);
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      document.removeEventListener("pointerdown", onPointerDown);
    };
  }, [menuOpen]);

  // Bash, then every CLI harness this session can run — the same list the Create pane offers.
  const menuKinds = [
    BASH_KIND,
    ...(sessionType === TOOL_SESSION_TYPE ? [] : CLI_HARNESS_OPTIONS.map(({ id }) => id)),
  ];

  return (
    <div ref={newTabRef} className="relative flex-shrink-0">
      <button
        ref={newTabButtonRef}
        type="button"
        data-testid="sessions-terminal-tab-new"
        aria-label="Open a new terminal"
        aria-haspopup="menu"
        aria-expanded={menuOpen}
        onClick={() => {
          // Sessions come and go outside tddy, so each opening lists the host's tmux sessions afresh.
          if (!menuOpen) tmux?.refresh();
          setMenuOpen(!menuOpen);
        }}
        className="px-2 py-1.5 text-muted-foreground hover:text-foreground"
      >
        <Plus className="h-3.5 w-3.5" />
      </button>
      {menuOpen ? (
        <div
          role="menu"
          aria-label="New terminal"
          className="absolute left-0 z-20 mt-1 min-w-[10rem] rounded-md border border-border bg-popover py-1 text-popover-foreground shadow-md"
        >
          <ul role="group" data-testid="sessions-terminal-tab-new-menu">
            {menuKinds.map((kind) => (
              <li key={kind} role="none">
                <button
                  type="button"
                  role="menuitem"
                  data-testid={`sessions-terminal-tab-new-${kind}`}
                  onClick={() => {
                    setMenuOpen(false);
                    onOpen(kind);
                  }}
                  className="block w-full px-3 py-2 text-left text-sm hover:bg-muted"
                >
                  {terminalKindLabel(kind)}
                </button>
              </li>
            ))}
          </ul>
          {tmux ? (
            <TmuxMenuSection
              tmux={tmux}
              onChoose={(name) => {
                setMenuOpen(false);
                onOpen(TMUX_KIND, name);
              }}
            />
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

/** The tmux section of the "+" menu: one item per session the host listed, or what stands in for them. */
function TmuxMenuSection({
  tmux,
  onChoose,
}: {
  tmux: TmuxMenuState;
  onChoose: (name: string) => void;
}) {
  return (
    <ul role="group" aria-label="tmux sessions" className="mt-1 border-t border-border pt-1">
      {tmuxRows(tmux, onChoose)}
    </ul>
  );
}

/**
 * The rows of the tmux section. A listing's error is shown in place of the sessions, and a listing
 * still in flight is shown as loading rather than as an empty host.
 */
function tmuxRows(tmux: TmuxMenuState, onChoose: (name: string) => void): React.ReactNode {
  if (tmux.error !== null) {
    return (
      <li
        role="none"
        data-testid="sessions-terminal-tab-new-tmux-error"
        className="px-3 py-2 text-xs text-destructive"
      >
        {tmux.error}
      </li>
    );
  }
  if (tmux.sessions.length > 0) {
    return tmux.sessions.map((session) => (
      <TmuxSessionRow key={session.name} session={session} onChoose={onChoose} />
    ));
  }
  if (tmux.loading) {
    return (
      <li
        role="none"
        data-testid="sessions-terminal-tab-new-tmux-loading"
        className="px-3 py-2 text-xs text-muted-foreground"
      >
        Loading tmux sessions…
      </li>
    );
  }
  return (
    <li
      role="none"
      data-testid="sessions-terminal-tab-new-tmux-empty"
      className="px-3 py-2 text-xs text-muted-foreground"
    >
      No tmux sessions
    </li>
  );
}

function TmuxSessionRow({
  session,
  onChoose,
}: {
  session: TmuxSessionInfo;
  onChoose: (name: string) => void;
}) {
  return (
    <li role="none">
      <button
        type="button"
        role="menuitem"
        data-testid={`sessions-terminal-tab-new-tmux-${session.name}`}
        onClick={() => onChoose(session.name)}
        className="block w-full px-3 py-2 text-left text-sm hover:bg-muted"
      >
        <span className="block">{session.name}</span>
        <span className="block text-xs text-muted-foreground">{tmuxSessionDetail(session)}</span>
      </button>
    </li>
  );
}

/** What the listing says about a session beside its name: its window count, and its attached clients when any. */
function tmuxSessionDetail({ windows, attached }: TmuxSessionInfo): string {
  const windowCount = windows === 1 ? "1 window" : `${windows} windows`;
  return attached > 0 ? `${windowCount} · ${attached} attached` : windowCount;
}
