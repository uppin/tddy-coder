import React from "react";
import { useAuthContext } from "../../hooks/authProvider";
import { DaemonNavMenu } from "./DaemonNavMenu";
import { DaemonSelectorConnected } from "./DaemonSelector";
import { UserAvatar } from "../UserAvatar";

const screenShellClassName =
  "min-h-svh w-full min-w-0 box-border px-4 py-6 sm:px-6 font-sans text-foreground";

export interface AppShellProps {
  /** Screen title shown in the header. */
  title: React.ReactNode;
  /** Hash-router navigation callback, forwarded to the navigation menu. */
  onNavigate: (path: string) => void;
  /**
   * `scroll` (default) — a padded, min-height page for list/form screens.
   * `fullbleed` — a fixed-height, overflow-hidden shell for drawer screens whose body owns scrolling.
   */
  variant?: "scroll" | "fullbleed";
  /** Optional extra header controls placed left of the daemon selector. */
  headerRight?: React.ReactNode;
  /** Test id applied to the shell root element. */
  dataTestId?: string;
  children: React.ReactNode;
}

/**
 * The unified header + layout shell shared by every daemon-mode screen. Owns the hamburger
 * navigation menu, the screen title, the daemon selector, and the user avatar, so individual
 * screens only supply their title and body.
 */
export function AppShell({
  title,
  onNavigate,
  variant = "scroll",
  headerRight,
  dataTestId,
  children,
}: AppShellProps) {
  const { user, logout } = useAuthContext();

  if (variant === "fullbleed") {
    return (
      <div
        data-testid={dataTestId}
        className="flex flex-col h-[100dvh] w-full overflow-hidden font-sans text-foreground"
      >
        <div className="flex items-center gap-3 px-2 py-1 border-b border-border flex-shrink-0">
          <DaemonNavMenu onNavigate={onNavigate} />
          <h1 className="text-sm font-semibold truncate">{title}</h1>
          <div className="flex-1" />
          {headerRight}
          <DaemonSelectorConnected />
          {user ? <UserAvatar user={user} onLogout={logout} /> : null}
        </div>
        <div className="flex flex-col flex-1 min-h-0 overflow-hidden">{children}</div>
      </div>
    );
  }

  return (
    <div data-testid={dataTestId} className={screenShellClassName}>
      <div className="flex items-center gap-3 mb-6">
        <DaemonNavMenu onNavigate={onNavigate} />
        <h1 className="text-xl font-bold flex-1">{title}</h1>
        {headerRight}
        <DaemonSelectorConnected />
        {user ? <UserAvatar user={user} onLogout={logout} /> : null}
      </div>
      {children}
    </div>
  );
}
