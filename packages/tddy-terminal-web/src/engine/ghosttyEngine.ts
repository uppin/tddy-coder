/**
 * `TerminalEngine` on ghostty-web. Holds every ghostty-web import and cast and the translation of
 * ghostty's bottom-based viewport (`viewportY`: 0 = live tip) into the interface's top-based
 * positions (`viewportTop = scrollbackLength - viewportY`).
 */

import { FitAddon, Terminal } from "ghostty-web";
import { newGhosttyInstance } from "../lib/ghosttyInstance";
import { watchDevicePixelRatio } from "../lib/terminalDprWatch";
import type {
  EngineBufferLine,
  EngineDisposable,
  TerminalEngine,
  TerminalEngineFactory,
  TerminalEngineOptions,
  TerminalEngineSize,
} from "./TerminalEngine";

/** A set of listeners the engine fans events out to, so they can be registered before `open`. */
class Listeners<T> {
  private readonly all = new Set<(value: T) => void>();

  add(listener: (value: T) => void): EngineDisposable {
    this.all.add(listener);
    return { dispose: () => void this.all.delete(listener) };
  }

  emit(value: T): void {
    for (const listener of [...this.all]) listener(value);
  }

  clear(): void {
    this.all.clear();
  }
}

class GhosttyEngine implements TerminalEngine {
  private term: Terminal | null = null;
  private fitAddon: FitAddon | null = null;
  private disposed = false;
  private readonly cleanups: Array<() => void> = [];
  private alternate = false;

  private readonly data = new Listeners<string>();
  private readonly resizes = new Listeners<TerminalEngineSize>();
  private readonly bells = new Listeners<void>();
  private readonly titles = new Listeners<string>();
  private readonly scrolls = new Listeners<number>();
  private readonly bufferChanges = new Listeners<boolean>();

  constructor(private readonly options: TerminalEngineOptions) {}

  async open(container: HTMLElement): Promise<void> {
    // Each terminal gets its own WASM instance rather than ghostty-web's process-wide one —
    // see `newGhosttyInstance` for why.
    const ghostty = await newGhosttyInstance();
    if (this.disposed) return;

    const term = new Terminal({
      ghostty,
      cols: this.options.cols,
      rows: this.options.rows,
      fontSize: this.options.fontSize,
      fontFamily: this.options.fontFamily,
      theme: this.options.theme,
      scrollback: this.options.scrollback,
    });
    this.term = term;

    const subscriptions = [
      term.onData((value) => this.data.emit(value)),
      term.onResize((size) => this.resizes.emit({ cols: size.cols, rows: size.rows })),
      term.onBell(() => this.bells.emit(undefined)),
      term.onTitleChange((title) => this.titles.emit(title)),
      // ghostty counts from the bottom; the interface counts from the top.
      term.onScroll(() => this.scrolls.emit(this.viewportTop())),
    ];
    this.cleanups.push(() => subscriptions.forEach((subscription) => subscription.dispose()));

    term.open(container);

    // ghostty-web 0.4.0 reads devicePixelRatio once, at renderer construction, and never again — a
    // terminal mounted on one display keeps that display's scale factor forever, so dragging the
    // window onto a screen with a different one leaves the canvas rendering at the old scale,
    // upscaled soft by the compositor. Sync the renderer to the live value and watch for changes:
    // render() detects the backing-store mismatch a DPR change leaves and fully repaints at the new
    // scale. (devicePixelRatio is private in ghostty-web's types, so the field is reached through a
    // cast, as the library gives no API.)
    const syncRendererDpr = () => {
      const renderer = term.renderer;
      if (!renderer || !term.wasmTerm) return;
      const dpr = window.devicePixelRatio;
      const mutable = renderer as unknown as { devicePixelRatio: number };
      if (mutable.devicePixelRatio === dpr) return;
      mutable.devicePixelRatio = dpr;
      renderer.render(term.wasmTerm, true, term.viewportY, term);
    };
    syncRendererDpr();
    this.cleanups.push(watchDevicePixelRatio(window, syncRendererDpr));

    // Only used to propose a grid: the components size the terminal themselves.
    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    this.fitAddon = fitAddon;
    this.alternate = this.alternateScreen();
  }

  dispose(): void {
    this.disposed = true;
    for (const cleanup of this.cleanups.splice(0)) cleanup();
    this.fitAddon?.dispose();
    this.fitAddon = null;
    // ghostty-web's dispose removes the canvas and textarea it appended. Its `element` is the
    // container it was opened on, which the caller owns, so it stays in the document.
    const term = this.term;
    this.term = null;
    term?.dispose();
    for (const listeners of [this.data, this.resizes, this.bells, this.titles, this.scrolls, this.bufferChanges]) {
      listeners.clear();
    }
  }

  // The cell grid is the canvas: geometry and pointer code measure it, not the full-width root.
  get screenElement(): HTMLElement | null {
    return this.term?.element?.querySelector("canvas") ?? null;
  }

  get inputElement(): HTMLTextAreaElement | null {
    return this.term?.textarea ?? null;
  }

  write(data: string | Uint8Array): void {
    this.term?.write(data);
    this.announceBufferSwitch();
  }

  paste(text: string): void {
    this.term?.paste(text);
  }

  clear(): void {
    this.term?.clear();
  }

  focus(): void {
    const term = this.term;
    if (!term) return;
    const textarea = term.textarea;
    if (textarea) {
      // ghostty-web routes keyboard input through this off-screen textarea — clicking the terminal
      // focuses it (see the library's canvas mousedown handler), and keydown bubbles from it to the
      // contenteditable host that owns the key listener. Focusing it directly is what makes the
      // terminal keyboard-ready. `term.focus()` instead focuses the host wrapper and re-asserts
      // that focus on a timer, so it can't be used to land focus on the input.
      textarea.removeAttribute("readonly");
      textarea.focus();
      return;
    }
    term.focus();
  }

  get cols(): number {
    return this.term?.cols ?? this.options.cols;
  }

  get rows(): number {
    return this.term?.rows ?? this.options.rows;
  }

  resize(cols: number, rows: number): void {
    this.term?.resize(cols, rows);
  }

  proposeDimensions(): TerminalEngineSize | null {
    return this.fitAddon?.proposeDimensions() ?? null;
  }

  setFontSize(px: number): void {
    if (this.term) this.term.options.fontSize = px;
  }

  fontSize(): number {
    return this.term?.options.fontSize ?? this.options.fontSize;
  }

  scrollbackLength(): number {
    return this.term?.getScrollbackLength() ?? 0;
  }

  viewportTop(): number {
    const term = this.term;
    if (!term) return 0;
    const scrollback = term.getScrollbackLength();
    const fromBottom = Math.max(0, Math.min(scrollback, Math.floor(term.getViewportY())));
    return scrollback - fromBottom;
  }

  scrollToLine(line: number): void {
    const term = this.term;
    if (!term) return;
    const scrollback = term.getScrollbackLength();
    const top = Math.max(0, Math.min(scrollback, line));
    // ghostty's own `scrollToLine` takes its bottom-based position.
    term.scrollToLine(scrollback - top);
  }

  scrollLines(amount: number): void {
    this.term?.scrollLines(amount);
  }

  scrollToTop(): void {
    this.term?.scrollToTop();
  }

  scrollToBottom(): void {
    this.term?.scrollToBottom();
  }

  mouseTracking(): boolean {
    return this.term?.hasMouseTracking() ?? false;
  }

  alternateScreen(): boolean {
    // The alternate-screen flag lives on the wasm terminal, not on the JS `Terminal`.
    const wasm = this.term?.wasmTerm as unknown as { isAlternateScreen?: () => boolean } | undefined;
    return wasm?.isAlternateScreen?.() ?? false;
  }

  readLines(): EngineBufferLine[] {
    const buffer = this.term?.buffer?.active;
    if (!buffer) return [];
    const lines: EngineBufferLine[] = [];
    for (let y = 0; y < buffer.length; y++) {
      const line = buffer.getLine(y);
      if (!line) continue;
      let hasInverse = false;
      for (let x = 0; x < line.length; x++) {
        if (line.getCell(x)?.isInverse()) {
          hasInverse = true;
          break;
        }
      }
      lines.push({ text: line.translateToString(true), hasInverse });
    }
    return lines;
  }

  cellInverse(line: number, col: number): boolean {
    return Boolean(this.term?.buffer?.active?.getLine(line)?.getCell(col)?.isInverse());
  }

  onData(listener: (data: string) => void): EngineDisposable {
    return this.data.add(listener);
  }

  onResize(listener: (size: TerminalEngineSize) => void): EngineDisposable {
    return this.resizes.add(listener);
  }

  onBell(listener: () => void): EngineDisposable {
    return this.bells.add(() => listener());
  }

  onTitleChange(listener: (title: string) => void): EngineDisposable {
    return this.titles.add(listener);
  }

  onScroll(listener: (viewportTop: number) => void): EngineDisposable {
    return this.scrolls.add(listener);
  }

  onBufferChange(listener: (alternate: boolean) => void): EngineDisposable {
    return this.bufferChanges.add(listener);
  }

  /** ghostty-web declares a buffer-change event but never fires it, so a switch is detected here:
   *  the only thing that moves the screen between buffers is output. */
  private announceBufferSwitch(): void {
    const now = this.alternateScreen();
    if (now === this.alternate) return;
    this.alternate = now;
    this.bufferChanges.emit(now);
  }
}

export const ghosttyEngine: TerminalEngineFactory = (options: TerminalEngineOptions): TerminalEngine =>
  new GhosttyEngine(options);
