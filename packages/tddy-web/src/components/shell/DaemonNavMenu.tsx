import { useEffect, useRef, useState } from "react";
import { Menu } from "lucide-react";
import { Button } from "@/components/ui/button";

/**
 * Hamburger menu for daemon-mode shell: Sessions (/) and Worktrees (/worktrees).
 */
export function DaemonNavMenu({
  onNavigate,
}: {
  onNavigate: (path: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  const go = (path: string) => {
    onNavigate(path);
    setOpen(false);
  };

  return (
    <div ref={ref} className="relative inline-block shrink-0">
      <Button
        type="button"
        variant="outline"
        size="icon"
        aria-label="Open navigation menu"
        aria-expanded={open}
        aria-haspopup="menu"
        data-testid="shell-menu-button"
        onClick={() => setOpen((o) => !o)}
      >
        <Menu className="size-5" aria-hidden />
      </Button>
      {open ? (
        <div
          role="menu"
          className="absolute top-full left-0 z-[1000] mt-1 min-w-[12rem] overflow-hidden rounded-md border border-border bg-popover p-1 text-popover-foreground shadow-md"
        >
          <Button
            type="button"
            variant="ghost"
            className="h-auto w-full justify-start rounded-sm px-3 py-2 font-normal"
            role="menuitem"
            data-testid="shell-menu-sessions"
            onClick={() => go("/")}
          >
            Sessions
          </Button>
          <Button
            type="button"
            variant="ghost"
            className="h-auto w-full justify-start rounded-sm px-3 py-2 font-normal"
            role="menuitem"
            data-testid="shell-menu-worktrees"
            onClick={() => go("/worktrees")}
          >
            Worktrees
          </Button>
        </div>
      ) : null}
    </div>
  );
}
