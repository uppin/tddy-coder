import { useEffect, useRef, useState } from "react";
import { Menu } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useCapabilityAvailability } from "@/hooks/useCapabilityAvailability";
import { useHostConnection } from "@/rpc/connections/registry";
import { useSelectedDaemon } from "@/rpc/selectedDaemon";

/**
 * Hamburger menu for the daemon-mode shell: Sessions, Worktrees, Tasks, Projects, Models & Agents,
 * VMs, LiveKit, the RPC Playground, and the serving daemon's own Settings.
 *
 * The LiveKit entry is offered only where the screen behind it has something to say. It is
 * *removed* rather than disabled: everything on that screen — the roster and the room list — is
 * presence, so on a wire that carries none it would lead somewhere empty, and an entry like that
 * invites a support question with no good answer (PRD AC 4). The route itself stays reachable, and
 * `LiveKitAppPage` explains itself to anyone who arrives by link.
 *
 * The entry and the screen read the same rule (`useCapabilityAvailability`) so they cannot
 * disagree: a room still joining, or one that failed with a reason, keeps the entry — the screen
 * has a join to report on, and the reason a join failed is exactly what an operator would go there
 * to find.
 */
export function DaemonNavMenu({
  onNavigate,
}: {
  onNavigate: (path: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const { selectedInstanceId } = useSelectedDaemon();
  const connection = useHostConnection(selectedInstanceId);
  const liveKitApplies = useCapabilityAvailability(connection, "presence") !== "unavailable";

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
        size="icon-xs"
        aria-label="Open navigation menu"
        aria-expanded={open}
        aria-haspopup="menu"
        data-testid="shell-menu-button"
        onClick={() => setOpen((o) => !o)}
      >
        <Menu className="size-4" aria-hidden />
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
            onClick={() => go("/sessions")}
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
          <Button
            type="button"
            variant="ghost"
            className="h-auto w-full justify-start rounded-sm px-3 py-2 font-normal"
            role="menuitem"
            data-testid="shell-menu-tasks"
            onClick={() => go("/tasks")}
          >
            Tasks
          </Button>
          <Button
            type="button"
            variant="ghost"
            className="h-auto w-full justify-start rounded-sm px-3 py-2 font-normal"
            role="menuitem"
            data-testid="shell-menu-projects"
            onClick={() => go("/projects")}
          >
            Projects
          </Button>
          <Button
            type="button"
            variant="ghost"
            className="h-auto w-full justify-start rounded-sm px-3 py-2 font-normal"
            role="menuitem"
            data-testid="shell-menu-models"
            onClick={() => go("/models")}
          >
            Models &amp; Agents
          </Button>
          <Button
            type="button"
            variant="ghost"
            className="h-auto w-full justify-start rounded-sm px-3 py-2 font-normal"
            role="menuitem"
            data-testid="shell-menu-vms"
            onClick={() => go("/vms")}
          >
            VMs
          </Button>
          {liveKitApplies && (
            <Button
              type="button"
              variant="ghost"
              className="h-auto w-full justify-start rounded-sm px-3 py-2 font-normal"
              role="menuitem"
              data-testid="shell-menu-livekit"
              onClick={() => go("/livekit")}
            >
              LiveKit
            </Button>
          )}
          <Button
            type="button"
            variant="ghost"
            className="h-auto w-full justify-start rounded-sm px-3 py-2 font-normal"
            role="menuitem"
            data-testid="shell-menu-rpc-playground"
            onClick={() => go("/rpc-playground")}
          >
            RPC Playground
          </Button>
          <Button
            type="button"
            variant="ghost"
            className="h-auto w-full justify-start rounded-sm px-3 py-2 font-normal"
            role="menuitem"
            data-testid="shell-menu-settings"
            onClick={() => go("/settings")}
          >
            Settings
          </Button>
        </div>
      ) : null}
    </div>
  );
}
