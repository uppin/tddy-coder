import { useCallback, useEffect, useRef, type RefObject } from "react";

export interface AgentTerminalFocusInput {
  /** True when this runtime is the focused (CSS-visible) one. */
  focused: boolean;
  /** True when the Agent pane holds the pane stack — no child, conversation or bash tab is up. */
  agentPaneActive: boolean;
  /** The pane stack is (or contains) the fullscreen element — see `useSessionPaneFullscreen`. */
  fullscreenActive: boolean;
}

export interface AgentTerminalFocus {
  /** The runtime's outer container — the focus guard's "inside me" boundary. */
  readonly containerRef: RefObject<HTMLDivElement | null>;
  /** Handed to the Agent terminal, which registers how to focus itself once it mounts. */
  readonly registerAgentFocus: (focus: () => void) => void;
}

/**
 * Keeps keyboard focus on a foreground runtime's Agent terminal: on selection, across a fullscreen
 * transition, and against a background session's terminal that steals it.
 */
export function useAgentTerminalFocus({
  focused,
  agentPaneActive,
  fullscreenActive,
}: AgentTerminalFocusInput): AgentTerminalFocus {
  // Imperative focus handle for the Agent pane's terminal. Each terminal self-focuses once at
  // mount, so first-selection works on its own; re-selecting an already-mounted runtime only flips
  // CSS visibility, so we replay focus here when this runtime comes to the foreground.
  const focusAgentTerminalRef = useRef<(() => void) | null>(null);
  const registerAgentFocus = useCallback((focus: () => void) => {
    focusAgentTerminalRef.current = focus;
  }, []);
  // The runtime's outer container — used by the focus guard to tell "focus landed inside me" from
  // "a sibling session's terminal stole focus".
  const containerRef = useRef<HTMLDivElement>(null);

  // When this runtime becomes focused with the Agent pane active, return keyboard focus to its
  // terminal — so selection alone makes the session ready to type, no click required. Never steals
  // focus for a backgrounded runtime, and stays out of the way when a bash tab or child pane is up.
  // TODO: `GrpcSessionTerminal` doesn't yet plumb a focus handle through to the terminal, so
  // focus-on-select only works for a session carried over its own room.
  useEffect(() => {
    if (focused && agentPaneActive) {
      focusAgentTerminalRef.current?.();
    }
  }, [focused, agentPaneActive]);

  // Focus guard: a backgrounded session keeps its terminal mounted, and a terminal that opens (or
  // is re-selected) while still transiently visible auto-focuses itself — ghostty-web focuses on
  // open and re-asserts it on a deferred timer. That lets a background session's terminal steal
  // keyboard focus from the foreground one a beat after selection. While this runtime is the
  // foreground one with its Agent pane active, reclaim focus whenever it lands in a *different*
  // session's terminal (never for focus that legitimately moves to the drawer, inspector, etc.).
  useEffect(() => {
    if (!focused || !agentPaneActive) return;
    const self = containerRef.current;
    if (!self) return;
    const onFocusIn = (e: FocusEvent) => {
      const target = e.target as Node | null;
      if (!target || self.contains(target)) return;
      const stealer = (target as HTMLElement).closest?.(
        "[data-testid^='sessions-runtime-terminal-']",
      );
      if (stealer) focusAgentTerminalRef.current?.();
    };
    document.addEventListener("focusin", onFocusIn, true);
    return () => document.removeEventListener("focusin", onFocusIn, true);
  }, [focused, agentPaneActive]);

  // A fullscreen transition re-lays-out the terminal and can drop keyboard focus on the way in or
  // out; the focus-on-select effect above is keyed on selection, so it never fires here. Replay it
  // so a terminal that fills the screen is immediately typeable.
  useEffect(() => {
    if (focused && agentPaneActive) focusAgentTerminalRef.current?.();
  }, [fullscreenActive, focused, agentPaneActive]);

  return { containerRef, registerAgentFocus };
}
