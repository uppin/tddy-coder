import { useEffect } from "react";
import type { ContainerRef, OnDataRef, TerminalRef } from "./terminalRefs";

/**
 * Desktop key combinations the browser would otherwise swallow, forwarded to the PTY while the
 * terminal is focused.
 */
export function useDesktopKeyForwarding({
  containerRef,
  onDataRef,
  sessionActive,
}: {
  containerRef: ContainerRef;
  onDataRef: OnDataRef;
  sessionActive: boolean;
}): void {
  // Desktop key combinations the browser would otherwise swallow (Shift+Tab → focus
  // traversal; Alt+letter → menu mnemonics) but which terminal apps expect. When the
  // terminal is focused, forward them to the PTY as the equivalent escape sequences.
  useEffect(() => {
    if (!sessionActive) return;
    const onKeyDown = (e: KeyboardEvent) => {
      const send = onDataRef.current;
      if (!send) return;
      const container = containerRef.current;
      if (!container || !container.contains(document.activeElement)) return;

      // Shift+Tab → reverse tab (CSI Z).
      if (e.key === "Tab" && e.shiftKey && !e.ctrlKey && !e.altKey && !e.metaKey) {
        e.preventDefault();
        send("\x1b[Z");
        return;
      }

      // Alt+<letter> → meta-sends-escape (ESC followed by the key).
      if (e.altKey && !e.ctrlKey && !e.metaKey && e.key.length === 1) {
        e.preventDefault();
        send("\x1b" + e.key);
      }
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [sessionActive]);
}

/** Forwards Escape pressed during IME composition, which the library drops. */
export function useImeEscapeForwarding({
  containerRef,
  termRef,
  onDataRef,
  ready,
  sessionActive,
}: {
  containerRef: ContainerRef;
  termRef: TerminalRef;
  onDataRef: OnDataRef;
  ready: boolean;
  sessionActive: boolean;
}): void {
  /**
   * ghostty-web ignores keydown while IME/composition is active (`isComposing`, `keyCode === 229`).
   * Typing `/` can start composition in some locales; Escape then cancels IME but never reaches the
   * terminal. Capture Escape on the hidden textarea and forward `\x1b` when the library would drop it.
   */
  useEffect(() => {
    if (!ready || !sessionActive) return;
    const container = containerRef.current;
    if (!container) return;

    const onEscapeDuringIme = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      const ta = termRef.current?.inputElement;
      if (!ta || e.target !== ta) return;
      if (!e.isComposing && e.keyCode !== 229) return;
      const send = onDataRef.current;
      if (!send) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      send("\x1b");
    };
    container.addEventListener("keydown", onEscapeDuringIme, true);
    return () => container.removeEventListener("keydown", onEscapeDuringIme, true);
  }, [ready, sessionActive]);
}
