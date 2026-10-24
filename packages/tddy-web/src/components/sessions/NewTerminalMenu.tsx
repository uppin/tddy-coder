import React, { useEffect, useRef, useState } from "react";
import { Plus } from "lucide-react";
import {
  BASH_KIND,
  CLI_HARNESS_OPTIONS,
  TOOL_SESSION_TYPE,
  terminalKindLabel,
} from "./cliHarnessOptions";

interface NewTerminalMenuProps {
  /** The session's type: a tool session offers bash only. Unset offers every harness. */
  sessionType?: string;
  /** Open a terminal of the chosen kind: `bash`, or a CLI harness's session type. */
  onOpen: (kind: string) => void;
}

/**
 * The tab strip's "+" control: a menu of bash and every CLI harness this session can run — the same
 * list the Create pane offers. Rendered outside the tab scroller so the menu is not clipped by it.
 */
export function NewTerminalMenu({ sessionType, onOpen }: NewTerminalMenuProps) {
  const [menuOpen, setMenuOpen] = useState(false);
  const newTabRef = useRef<HTMLDivElement>(null);
  const newTabButtonRef = useRef<HTMLButtonElement>(null);

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
        onClick={() => setMenuOpen((open) => !open)}
        className="px-2 py-1.5 text-muted-foreground hover:text-foreground"
      >
        <Plus className="h-3.5 w-3.5" />
      </button>
      {menuOpen ? (
        <ul
          role="menu"
          data-testid="sessions-terminal-tab-new-menu"
          className="absolute left-0 z-20 mt-1 min-w-[10rem] rounded-md border border-border bg-popover py-1 text-popover-foreground shadow-md"
        >
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
      ) : null}
    </div>
  );
}
