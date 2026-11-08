/**
 * The terminal emulator behind the web terminal components.
 *
 * The components talk to an engine only through this interface, so the emulator library can be
 * swapped without touching them. Every scroll position here counts **from the top** of the
 * buffer: line 0 is the oldest retained scrollback line, and the viewport is at the live tip when
 * `viewportTop() === scrollbackLength()`.
 *
 * PRD: packages/tddy-terminal-web/docs/terminal-engine.md
 */

/** Releases a listener registered on an engine. */
export interface EngineDisposable {
  dispose(): void;
}

export interface TerminalEngineTheme {
  background?: string;
  foreground?: string;
}

/** What an engine is created with; everything else is set through the engine afterwards. */
export interface TerminalEngineOptions {
  cols: number;
  rows: number;
  fontSize: number;
  fontFamily?: string;
  theme?: TerminalEngineTheme;
  /** Lines of scrollback retained above the screen. */
  scrollback?: number;
}

/** One buffer line as read back from the engine. */
export interface EngineBufferLine {
  /** The line's text with trailing whitespace trimmed. */
  text: string;
  /** True when any cell on the line is drawn in inverse video. */
  hasInverse: boolean;
}

export interface TerminalEngineSize {
  cols: number;
  rows: number;
}

export interface TerminalEngine {
  /** Attaches the engine to `container` and resolves once it renders and accepts input. */
  open(container: HTMLElement): Promise<void>;
  /** Detaches from the DOM and releases every resource and listener. */
  dispose(): void;

  /** The element the engine renders the cell grid into — what geometry and pointer code measure. */
  readonly screenElement: HTMLElement | null;
  /** The engine's keyboard input element, when it has one. */
  readonly inputElement: HTMLTextAreaElement | null;

  write(data: string | Uint8Array): void;
  /** Pastes as typed input, bracketed when the running program asked for bracketed paste. */
  paste(text: string): void;
  clear(): void;
  focus(): void;

  readonly cols: number;
  readonly rows: number;
  resize(cols: number, rows: number): void;
  /** The grid that fits the container at the current font size, or null when it cannot measure yet. */
  proposeDimensions(): TerminalEngineSize | null;
  setFontSize(px: number): void;
  fontSize(): number;

  /** Lines of scrollback above the screen (0 when nothing has scrolled off, and on the alternate screen). */
  scrollbackLength(): number;
  /** The buffer line shown on the viewport's first row, counted from the top (0 = oldest line). */
  viewportTop(): number;
  /** Moves the viewport so `line` (top-based) is its first row, clamped to the buffer. */
  scrollToLine(line: number): void;
  /** Relative scroll: positive moves toward the live tip, negative into the scrollback. */
  scrollLines(amount: number): void;
  scrollToTop(): void;
  scrollToBottom(): void;

  /** True while the running program has asked for mouse reports. */
  mouseTracking(): boolean;
  /** True while the alternate screen is active. */
  alternateScreen(): boolean;

  /** Every buffer line, scrollback first. */
  readLines(): EngineBufferLine[];
  /** True when the cell at buffer line `line` (top-based), column `col` is drawn in inverse video. */
  cellInverse(line: number, col: number): boolean;

  onData(listener: (data: string) => void): EngineDisposable;
  onResize(listener: (size: TerminalEngineSize) => void): EngineDisposable;
  onBell(listener: () => void): EngineDisposable;
  onTitleChange(listener: (title: string) => void): EngineDisposable;
  /** Fires with the new top-based `viewportTop()` whenever the viewport moves. */
  onScroll(listener: (viewportTop: number) => void): EngineDisposable;
  /** Fires when the active buffer switches between the normal and the alternate screen. */
  onBufferChange(listener: (alternate: boolean) => void): EngineDisposable;
}

/** Creates an engine; nothing is rendered until `open`. */
export type TerminalEngineFactory = (options: TerminalEngineOptions) => TerminalEngine;
