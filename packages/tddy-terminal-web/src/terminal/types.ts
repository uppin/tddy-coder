/** Public prop and handle types of `TerminalView` (re-exported from it). */

export interface TerminalViewTheme {
  background?: string;
  foreground?: string;
}

export interface TerminalViewProps {
  initialContent?: string;
  cols?: number;
  rows?: number;
  fontSize?: number;
  minFontSize?: number;
  maxFontSize?: number;
  fontFamily?: string;
  theme?: TerminalViewTheme;
  onData?: (data: string) => void;
  onResize?: (size: { cols: number; rows: number }) => void;
  onBell?: () => void;
  onTitleChange?: (title: string) => void;
  onReady?: () => void;
  /**
   * Fires with the new viewport top (the buffer line on its first row, counted from the top;
   * equal to the scrollback length when pinned to latest) whenever the viewport moves — wheel, touch, keyboard, or programmatic scroll. Used by
   * TerminalSession to mirror scroll position across the two overlay terminals.
   */
  onScroll?: (viewportTop: number) => void;
  /** When true, log write/onData and lifecycle to console. */
  debugLogging?: boolean;
  /** When true, prevent terminal from receiving focus on pointer/touch events (e.g. mobile when keyboard closed). */
  preventFocusOnTap?: boolean;
  /** When false, the backend session is gone — disable interaction and expose data-session-active for accessibility/tests. */
  sessionActive?: boolean;
  /**
   * When true (default), a two-finger pinch on the terminal adjusts font size (same as pitch in/out).
   * Single-finger touch still forwards SGR mouse when the TUI has mouse tracking enabled.
   */
  pinchZoomFont?: boolean;
  /**
   * Minimum CSS height for the terminal container. Default 200 (full-page embed). Use `0` for small fixed panes (e.g. floating overlay) so content is not clipped.
   */
  containerMinHeightPx?: number;
  /**
   * Keep an exact col×row grid; font size follows container size (resize the pane to scale). Disables FitAddon fitting and user font zoom shortcuts.
   */
  fixedViewportGrid?: { cols: number; rows: number };
  /**
   * Lines of scrollback to retain above the viewport. 0 = none (e.g. alternate buffer).
   * Non-zero enables native primary-screen scrollback; used by both the live terminal
   * (post-connect history) and the older-history page terminal (forward-filled pre-connect history).
   */
  scrollback?: number;
  /** `data-testid` for the terminal container. Defaults to `terminal`; the older-history
   *  terminal passes `terminal-older` so tests can target each instance separately. */
  testId?: string;
}

export interface BufferLineInfo {
  /** Plain text of this line (trailing whitespace trimmed). */
  text: string;
  /** True if any cell on this line has reverse-video (isInverse). */
  hasInverse: boolean;
}

export interface TerminalViewHandle {
  write(data: string | Uint8Array): void;
  clear(): void;
  focus(): void;
  getBufferText?(): string;
  /** Return per-line text + attribute info from the active buffer. */
  getBufferLines?(): BufferLineInfo[];
  /** Apply font size to the live terminal and refit (Green phase). */
  setTerminalFontSize?(px: number): void;
  /** The buffer line on the viewport's first row, counted from the top (0 = oldest line). */
  getViewportTop?(): number;
  /** Scroll the viewport to the top of the scrollback (no-op when scrollback is 0). */
  scrollToTop?(): void;
  /** Scroll the viewport to the bottom (latest output) — used after a page swap to land seamlessly. */
  scrollToBottom?(): void;
  /**
   * Scroll to an absolute line in the combined buffer (0 = top of scrollback,
   * scrollbackLength = bottom). The primary "full control" viewport API.
   */
  scrollToLine?(line: number): void;
  /** Relative scroll by `amount` lines (positive = down, negative = up). */
  scrollLines?(amount: number): void;
  /** Number of scrollback (history) lines, excluding the active screen. */
  getScrollbackLength?(): number;
  /** True when the viewport is pinned to the latest output (not scrolled up into scrollback). */
  isPinnedToBottom?(): boolean;
  /**
   * Native Scrollbar { total, offset, len } — single source of truth for viewport position,
   * same coordinate space as scrollToLine.
   */
  getScrollbar?(): { total: number; offset: number; len: number };
  /** True when the TUI has enabled mouse tracking (DEC 1006 / 1002). */
  hasMouseTracking?(): boolean;
  /** True when the terminal is in the alternate screen buffer (DEC 1049). */
  isAlternateScreen?(): boolean;
  /** Fires after written data has been parsed: terminal state (modes, buffers) reads current then. */
  onWriteParsed?(listener: () => void): { dispose(): void };
  /**
   * Simulate a keystroke: scroll to bottom first (native scroll-to-bottom.keystroke=true),
   * then forward via onData.
   */
  sendKeystroke?(data: string): void;
  /**
   * Paste `text` as the operator would: wrapped in `ESC [ 200 ~` … `ESC [ 201 ~` when the program
   * enabled bracketed paste (DEC 2004), plain otherwise — then forwarded via onData.
   */
  paste?(text: string): void;
}
