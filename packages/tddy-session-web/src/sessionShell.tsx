/**
 * The slot where the app's screen shell (header, navigation menu, daemon selector, user avatar) is
 * mounted around a session screen.
 *
 * The shell belongs to the application: it owns the navigation menu and the auth context. A session
 * screen only asks for it, so the host application provides a component through
 * {@link SessionShellProvider} and the screen renders {@link SessionShell}.
 */

import React, { createContext, useContext, type ComponentType } from "react";

export interface SessionShellProps {
  /** Screen title shown in the header. */
  title: React.ReactNode;
  /** Hash-router navigation callback, forwarded to the navigation menu. */
  onNavigate: (path: string) => void;
  /** `fullbleed` — a fixed-height, overflow-hidden shell for drawer screens whose body owns scrolling. */
  variant?: "scroll" | "fullbleed";
  /** Optional extra header controls placed left of the daemon selector. */
  headerRight?: React.ReactNode;
  /** Test id applied to the shell root element. */
  dataTestId?: string;
  /** The screen can do nothing without an RPC client that reaches the selected host. */
  requiresHost?: boolean;
  children: React.ReactNode;
}

const SessionShellContext = createContext<ComponentType<SessionShellProps> | null>(null);

/** Supplies the component that renders the app's screen shell around a session screen. */
export function SessionShellProvider({
  shell,
  children,
}: {
  shell: ComponentType<SessionShellProps>;
  children?: React.ReactNode;
}): React.ReactElement {
  return <SessionShellContext.Provider value={shell}>{children}</SessionShellContext.Provider>;
}

/** Mounts the provided shell. Throws without a {@link SessionShellProvider}: a screen with no shell has no navigation. */
export function SessionShell(props: SessionShellProps): React.ReactElement {
  const Shell = useContext(SessionShellContext);
  if (Shell === null) {
    throw new Error("SessionShell needs a SessionShellProvider above it");
  }
  return <Shell {...props} />;
}
