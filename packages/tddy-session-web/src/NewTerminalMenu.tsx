import React, { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Plus } from "lucide-react";
import type { TmuxSessionInfo } from "tddy-client-web/gen/terminal_session_pb";
import {
  BASH_KIND,
  TMUX_KIND,
  TMUX_NEW_KIND,
  TDDY_CODER_LAUNCH,
  cliSessionTypeOptions,
  knownSessionLaunch,
  knownSessionTypes,
  terminalKindLabel,
} from "tddy-session-create-web";
import { nextTmuxSessionName } from "./tmuxSessionName";
import { SANDBOX_WRAPPER_KIND, TARGET_WRAPPER_KIND, TMUX_WRAPPER_KIND } from "./useSessionTerminals";
import type { TmuxMenuState } from "./useTmuxSessions";

interface NewTerminalMenuProps {
  /** The session's type: a tool session offers bash only. Unset offers every harness. */
  sessionType?: string;
  /** Open a terminal of the chosen kind: `bash`, a CLI harness's session type, or `tmux` attached
   *  to `tmuxSession`; `wrappers` (outermost first) is what it runs wrapped in. */
  onOpen: (kind: string, tmuxSession?: string, wrappers?: readonly string[]) => void;
  /** The session this menu opens terminals for: its short id (the first segment of the id) names a
   *  new tmux session. */
  sessionId: string;
  /** The host's tmux sessions, offered as a section of the menu; absent offers no tmux section. The
   *  section lists again each time the menu opens. */
  tmux?: TmuxMenuState;
  /** Whether the session runs sandboxed (`SessionEntry.sandbox`): its shell opens "In sandbox" — in
   *  a jail of its own — by default, or "On host". Its agent kinds are listed under
   *  "In sandbox" too, each opening with `wrappers: [sandbox]` (`#session-liveness` 7/8). */
  sandboxed?: boolean;
  /** Whether the session runs on a target (`SessionEntry.target` is set): its shell opens "On
   *  target" — on that machine, in the session's worktree there — by default, or "On this host".
   *  Its agent kinds are listed under "On target" too, each opening with `wrappers: [target]`
   *  (`#session-liveness` 8/8). */
  onTarget?: boolean;
}

/** Gap between the "+" button and the top of its menu, in pixels. */
const MENU_GAP_PX = 4;

/** The menu's `min-w-[10rem]` in pixels: its width before it is measured. */
const MENU_MIN_WIDTH_PX = 160;

/** Smallest distance the menu keeps from the viewport's left and right edges, in pixels. */
const MENU_VIEWPORT_MARGIN_PX = 8;

/** Stacking of the portaled menu above the page: it lives in `document.body`, beside the app's panes. */
const MENU_Z_INDEX = 20;

/** Viewport coordinates of the menu's top-left corner. */
interface MenuPosition {
  top: number;
  left: number;
}

/**
 * The tab strip's "+" control: a menu of bash and every CLI harness this session can run — the same
 * list the Create pane offers — and, below them, the host's tmux sessions. The control is the last
 * tab of the tab scroller, sticky at its right edge; the menu is portaled to `document.body` so the
 * scroller does not clip it, and is placed under the button.
 *
 * The tmux section is a group of its own, so the bash and harness list stays exactly the terminals
 * it offers, whatever state the tmux listing is in.
 */
export function NewTerminalMenu({
  sessionType,
  onOpen,
  sessionId,
  tmux: tmuxState,
  sandboxed = false,
  onTarget = false,
}: NewTerminalMenuProps) {
  const [menuOpen, setMenuOpen] = useState(false);
  // The name typed for "New tmux session…", or null while that item is not open.
  const [tmuxNaming, setTmuxNaming] = useState<string | null>(null);
  const [menuPosition, setMenuPosition] = useState<MenuPosition | null>(null);
  const newTabRef = useRef<HTMLDivElement>(null);
  const newTabButtonRef = useRef<HTMLButtonElement>(null);
  const menuPanelRef = useRef<HTMLDivElement>(null);

  // A tddy-coder session's participant runs bash only, so there is no tmux server on its side to
  // list. Which sessions those are is the launch kind their daemon declared for the session type.
  const isTddyCoderSession = knownSessionLaunch(sessionType) === TDDY_CODER_LAUNCH;
  const tmux = isTddyCoderSession ? undefined : tmuxState;
  // A tddy-coder participant runs bash on its own side only: it opens no jail.
  const offersSandbox = sandboxed && !isTddyCoderSession;
  // A tddy-coder participant opens nothing on a target. A sandboxed session's shell is the jail's
  // choice, so a session offers one grouping or the other.
  const offersTarget = onTarget && !offersSandbox && !isTddyCoderSession;

  // Closing the menu drops an unsent name.
  useEffect(() => {
    if (!menuOpen) setTmuxNaming(null);
  }, [menuOpen]);

  // While the menu is open, it is placed under the button: now, and again whenever the viewport or
  // any scroll container moves the button. Listened for only while open, like the dismiss listeners.
  // A layout effect, so the menu is placed before the browser paints it. The button sits at the
  // strip's right edge, so the menu is kept inside the viewport instead of hanging off it.
  const menuMounted = menuPosition !== null;
  useLayoutEffect(() => {
    if (!menuOpen) return;
    const placeMenu = () => {
      const rect = newTabButtonRef.current?.getBoundingClientRect();
      if (!rect) return;
      const width = menuPanelRef.current?.offsetWidth ?? MENU_MIN_WIDTH_PX;
      const rightmost = window.innerWidth - width - MENU_VIEWPORT_MARGIN_PX;
      const top = rect.bottom + MENU_GAP_PX;
      const left = Math.max(MENU_VIEWPORT_MARGIN_PX, Math.min(rect.left, rightmost));
      setMenuPosition((was) => (was?.top === top && was.left === left ? was : { top, left }));
    };
    placeMenu();
    window.addEventListener("resize", placeMenu);
    window.addEventListener("scroll", placeMenu, true);
    return () => {
      window.removeEventListener("resize", placeMenu);
      window.removeEventListener("scroll", placeMenu, true);
    };
  }, [menuOpen, menuMounted]);

  // The "+" menu closes on Escape and on a press outside it. Both are listened for only while it is
  // open, so an idle tab strip holds no document listeners. The menu is portaled, so a press inside
  // it is checked against its own panel as well as against the control.
  useEffect(() => {
    if (!menuOpen) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      setMenuOpen(false);
      newTabButtonRef.current?.focus();
    };
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (target instanceof Node) {
        if (newTabRef.current?.contains(target)) return;
        if (menuPanelRef.current?.contains(target)) return;
      }
      setMenuOpen(false);
    };
    document.addEventListener("keydown", onKeyDown);
    document.addEventListener("pointerdown", onPointerDown);
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      document.removeEventListener("pointerdown", onPointerDown);
    };
  }, [menuOpen]);

  // Bash, then every CLI agent kind a daemon has served this client — the same list the
  // Create pane offers, registered from its `ListAgents` answers.
  const agentKinds = isTddyCoderSession
    ? []
    : cliSessionTypeOptions(knownSessionTypes()).map(({ id }) => id);
  const menuKinds = [BASH_KIND, ...agentKinds];

  return (
    <div ref={newTabRef} className="flex-shrink-0">
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
      {menuOpen && menuPosition
        ? createPortal(
            <div
              ref={menuPanelRef}
              role="menu"
              aria-label="New terminal"
              style={{
                position: "fixed",
                zIndex: MENU_Z_INDEX,
                top: menuPosition.top,
                left: menuPosition.left,
              }}
              className="min-w-[10rem] rounded-md border border-border bg-popover py-1 text-popover-foreground shadow-md"
            >
              {offersSandbox ? (
                <>
                  <ul role="group" aria-label="In sandbox" data-testid="sessions-terminal-tab-new-menu">
                    {menuKinds.map((kind) => (
                      <MenuKindItem
                        key={kind}
                        kind={kind}
                        testId={`sessions-terminal-tab-new-in-sandbox-${kind}`}
                        onChoose={() => {
                          setMenuOpen(false);
                          onOpen(kind, undefined, [SANDBOX_WRAPPER_KIND]);
                        }}
                      />
                    ))}
                  </ul>
                  <ul role="group" aria-label="On host" className="mt-1 border-t border-border pt-1">
                    {menuKinds.map((kind) => (
                      <MenuKindItem
                        key={kind}
                        kind={kind}
                        testId={`sessions-terminal-tab-new-on-host-${kind}`}
                        onChoose={() => {
                          setMenuOpen(false);
                          onOpen(kind);
                        }}
                      />
                    ))}
                  </ul>
                </>
              ) : offersTarget ? (
                <>
                  <ul role="group" aria-label="On target" data-testid="sessions-terminal-tab-new-menu">
                    {menuKinds.map((kind) => (
                      <MenuKindItem
                        key={kind}
                        kind={kind}
                        testId={`sessions-terminal-tab-new-on-target-${kind}`}
                        onChoose={() => {
                          setMenuOpen(false);
                          onOpen(kind, undefined, [TARGET_WRAPPER_KIND]);
                        }}
                      />
                    ))}
                  </ul>
                  <ul role="group" aria-label="On this host" className="mt-1 border-t border-border pt-1">
                    {menuKinds.map((kind) => (
                      <MenuKindItem
                        key={kind}
                        kind={kind}
                        testId={`sessions-terminal-tab-new-on-this-host-${kind}`}
                        onChoose={() => {
                          setMenuOpen(false);
                          onOpen(kind);
                        }}
                      />
                    ))}
                  </ul>
                </>
              ) : (
                <ul role="group" data-testid="sessions-terminal-tab-new-menu">
                  {menuKinds.map((kind) => (
                    <MenuKindItem
                      key={kind}
                      kind={kind}
                      testId={`sessions-terminal-tab-new-${kind}`}
                      onChoose={() => {
                        setMenuOpen(false);
                        onOpen(kind);
                      }}
                    />
                  ))}
                </ul>
              )}
              {agentKinds.length > 0 ? (
                <ul role="group" aria-label="Agents in tmux" className="mt-1 border-t border-border pt-1">
                  {agentKinds.map((kind) => (
                    <li key={kind} role="none">
                      <button
                        type="button"
                        role="menuitem"
                        data-testid={`sessions-terminal-tab-new-in-tmux-${kind}`}
                        onClick={() => {
                          setMenuOpen(false);
                          onOpen(kind, undefined, [TMUX_WRAPPER_KIND]);
                        }}
                        className="block w-full px-3 py-2 text-left text-sm hover:bg-muted"
                      >
                        {terminalKindLabel(kind)} in tmux
                      </button>
                    </li>
                  ))}
                </ul>
              ) : null}
              {tmux ? (
                <TmuxMenuSection
                  tmux={tmux}
                  onChoose={(name) => {
                    setMenuOpen(false);
                    onOpen(TMUX_KIND, name);
                  }}
                  naming={tmuxNaming}
                  onStartNaming={() =>
                    setTmuxNaming(
                      nextTmuxSessionName(
                        sessionId.split("-")[0],
                        tmux.sessions.map((session) => session.name),
                      ),
                    )
                  }
                  onNameChange={setTmuxNaming}
                  onSubmitName={(name) => {
                    setMenuOpen(false);
                    onOpen(TMUX_NEW_KIND, name);
                  }}
                  onCancelName={() => setTmuxNaming(null)}
                />
              ) : null}
            </div>,
            document.body,
          )
        : null}
    </div>
  );
}

/** One item of a "+" menu group: the terminal kind's label, opening it when chosen. */
function MenuKindItem({
  kind,
  testId,
  onChoose,
}: {
  kind: string;
  testId: string;
  onChoose: () => void;
}) {
  return (
    <li role="none">
      <button
        type="button"
        role="menuitem"
        data-testid={testId}
        onClick={onChoose}
        className="block w-full px-3 py-2 text-left text-sm hover:bg-muted"
      >
        {terminalKindLabel(kind)}
      </button>
    </li>
  );
}

/**
 * The tmux section of the "+" menu: "New tmux session…" first, then one item per session the host
 * listed, or what stands in for them. While `naming` is set, "New tmux session…" shows its name field:
 * Enter starts the session with the typed name, Escape cancels it and keeps the menu open.
 */
function TmuxMenuSection({
  tmux,
  onChoose,
  naming,
  onStartNaming,
  onNameChange,
  onSubmitName,
  onCancelName,
}: {
  tmux: TmuxMenuState;
  onChoose: (name: string) => void;
  naming: string | null;
  onStartNaming: () => void;
  onNameChange: (name: string) => void;
  onSubmitName: (name: string) => void;
  onCancelName: () => void;
}) {
  return (
    <ul role="group" aria-label="tmux sessions" className="mt-1 border-t border-border pt-1">
      <li role="none">
        {naming === null ? (
          <button
            type="button"
            role="menuitem"
            data-testid="sessions-terminal-tab-new-tmux-new"
            onClick={onStartNaming}
            className="block w-full px-3 py-2 text-left text-sm hover:bg-muted"
          >
            New tmux session…
          </button>
        ) : (
          <input
            type="text"
            autoFocus
            aria-label="New tmux session name"
            data-testid="sessions-terminal-tab-new-tmux-name"
            value={naming}
            onChange={(event) => onNameChange(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                const name = naming.trim();
                if (name !== "") onSubmitName(name);
              } else if (event.key === "Escape") {
                // Handled here so the menu's own Escape listener keeps the menu open.
                event.stopPropagation();
                onCancelName();
              }
            }}
            className="w-full px-3 py-2 text-sm bg-background"
          />
        )}
      </li>
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
