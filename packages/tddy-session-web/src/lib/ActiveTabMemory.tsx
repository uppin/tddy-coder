import React, { createContext, useContext, useRef, type ReactNode } from "react";

/** The tab each session was last on, by session id. */
export type ActiveTabMemory = Map<string, string>;

const ActiveTabMemoryContext = createContext<ActiveTabMemory | null>(null);

/**
 * Owns the tab each session was last on for the lifetime of the screen that mounts it. A Resume swaps
 * the stopped session's strip for the live one — a different component, so the strip's own state would
 * start over on the Agent tab — and the memory outlives that swap. It belongs to the screen, never to
 * the module: a module-level map would carry one screen's tabs into the next mount of the same session
 * id (`#session-liveness` 4/4).
 */
export function ActiveTabMemoryProvider({ children }: { children: ReactNode }) {
  const memory = useRef<ActiveTabMemory>(new Map());
  return (
    <ActiveTabMemoryContext.Provider value={memory.current}>{children}</ActiveTabMemoryContext.Provider>
  );
}

/** The memory of the enclosing {@link ActiveTabMemoryProvider}, or a private one under no provider. */
export function useActiveTabMemory(): ActiveTabMemory {
  const provided = useContext(ActiveTabMemoryContext);
  const own = useRef<ActiveTabMemory>(new Map());
  return provided ?? own.current;
}
