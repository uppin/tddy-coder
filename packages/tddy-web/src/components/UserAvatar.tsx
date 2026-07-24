import { useEffect, useRef, useState } from "react";
import type { GitHubUser } from "../gen/auth_pb";
import { Button } from "@/components/ui/button";
import { useAuthContext } from "../hooks/authProvider";

/**
 * Unobtrusive indicator shown in the top bar while the shared session store is minting a fresh
 * access token (e.g. right after the device wakes). Renders nothing when no refresh is in flight.
 */
function SessionRefreshIndicator() {
  const { isRefreshing } = useAuthContext();
  if (!isRefreshing) {
    return null;
  }
  return (
    <span
      data-testid="session-refreshing-indicator"
      className="text-xs text-muted-foreground"
      title="Refreshing session…"
    >
      Refreshing…
    </span>
  );
}

/**
 * Compact top-bar identity: just the avatar + username, acting as a trigger for a small profile
 * menu. "Sign out" lives inside that menu (click the username to reveal it) rather than sitting
 * permanently in the header.
 */
export function UserAvatar({
  user,
  onLogout,
}: {
  user: GitHubUser;
  onLogout: () => void;
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

  return (
    <div ref={ref} className="relative inline-flex items-center gap-2 shrink-0">
      <SessionRefreshIndicator />
      <button
        type="button"
        aria-label="Open profile menu"
        aria-expanded={open}
        aria-haspopup="menu"
        data-testid="user-profile-trigger"
        onClick={() => setOpen((o) => !o)}
        className="inline-flex items-center gap-2 rounded-md px-1.5 py-1 text-sm hover:bg-muted transition-colors"
      >
        <img
          data-testid="user-avatar"
          src={user.avatarUrl}
          alt={user.login}
          className="size-6 rounded-full"
        />
        <span data-testid="user-login" className="max-w-[10rem] truncate">
          {user.login}
        </span>
      </button>
      {open ? (
        <div
          role="menu"
          className="absolute top-full right-0 z-[1000] mt-1 min-w-[10rem] overflow-hidden rounded-md border border-border bg-popover p-1 text-popover-foreground shadow-md"
        >
          <Button
            type="button"
            variant="ghost"
            className="h-auto w-full justify-start rounded-sm px-3 py-2 font-normal"
            role="menuitem"
            data-testid="logout-button"
            onClick={() => {
              setOpen(false);
              onLogout();
            }}
          >
            Sign out
          </Button>
        </div>
      ) : null}
    </div>
  );
}
