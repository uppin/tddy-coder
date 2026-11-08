/**
 * `TerminalEngine` on xterm.js: WebGL renderer where the browser offers a WebGL context, xterm's
 * DOM renderer otherwise (developer-consented fallback, 2026-11-08). The active renderer is named
 * on xterm's root element inside the container as `data-terminal-renderer` (`webgl` | `dom`).
 *
 * xterm.js counts its viewport from the top already (`viewportY`, `baseY`), so the interface's
 * top-based positions map straight onto it. It also reports mouse input, wheel steps and
 * alternate-screen arrows to the running program itself, handles the device pixel ratio, and fires
 * a buffer-change event — none of that is done here.
 *
 * OSC 52 is not handled here: the components tap the output stream for it (`lib/osc52.ts`), which
 * also answers no clipboard reads.
 *
 * Docs: packages/tddy-terminal-web/docs/xterm-engine.md
 */

import "@xterm/xterm/css/xterm.css";
import { FitAddon } from "@xterm/addon-fit";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal, type IDisposable, type ITerminalAddon } from "@xterm/xterm";
import type {
  EngineBufferLine,
  EngineDisposable,
  TerminalEngine,
  TerminalEngineFactory,
  TerminalEngineOptions,
  TerminalEngineSize,
} from "./TerminalEngine";

type Renderer = "webgl" | "dom";

/** What the engine needs of the WebGL renderer addon: `@xterm/addon-webgl`'s `WebglAddon`. */
export interface WebglRendererAddon extends ITerminalAddon {
  /** Fires when the WebGL context is lost for good (the addon waits for a restore first). */
  onContextLoss(listener: () => void): IDisposable;
}

/** Builds the WebGL renderer addon; throws when it cannot (the addon throws on load without WebGL). */
export type WebglRendererFactory = () => WebglRendererAddon;

const createWebglAddon: WebglRendererFactory = () => new WebglAddon();

/** Loads the WebGL renderer; null when the browser gives no WebGL context. */
function tryLoadWebgl(
  term: Terminal,
  createWebgl: WebglRendererFactory,
  onContextLoss: () => void,
): WebglRendererAddon | null {
  let addon: WebglRendererAddon | null = null;
  try {
    addon = createWebgl();
    addon.onContextLoss(onContextLoss);
    term.loadAddon(addon);
    return addon;
  } catch {
    // No WebGL context: xterm keeps its DOM renderer.
    try {
      addon?.dispose();
    } catch {
      // A half-loaded addon that cannot be disposed holds nothing to release.
    }
    return null;
  }
}

class XtermEngine implements TerminalEngine {
  private term: Terminal | null = null;
  private fitAddon: FitAddon | null = null;
  private webgl: WebglRendererAddon | null = null;
  private disposed = false;
  private readonly subscriptions: EngineDisposable[] = [];

  /** Listeners registered before `open` are attached to the terminal as soon as it exists. */
  private readonly registrations: Array<(term: Terminal) => EngineDisposable> = [];

  constructor(
    private readonly options: TerminalEngineOptions,
    private readonly createWebgl: WebglRendererFactory,
  ) {}

  async open(container: HTMLElement): Promise<void> {
    if (this.disposed) return;
    const { cols, rows, fontSize, fontFamily, theme, scrollback } = this.options;
    const term = new Terminal({
      cols,
      rows,
      fontSize,
      // An option left out keeps xterm's default; passed as `undefined` it overrides it (an
      // undefined `scrollback` throws while the buffer is allocated).
      ...(fontFamily !== undefined && { fontFamily }),
      ...(theme !== undefined && { theme }),
      ...(scrollback !== undefined && { scrollback }),
      // The Unicode 11 width provider is a proposed API.
      allowProposedApi: true,
    });
    this.term = term;
    for (const register of this.registrations) this.subscriptions.push(register(term));

    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    this.fitAddon = fitAddon;

    // Wide characters (emoji, CJK) take two cells, as the TUI that drew them laid them out.
    term.loadAddon(new Unicode11Addon());
    term.unicode.activeVersion = "11";

    term.open(container);

    this.webgl = tryLoadWebgl(term, this.createWebgl, () => this.fallBackToDom());
    this.nameRenderer(this.webgl ? "webgl" : "dom");
  }

  dispose(): void {
    this.disposed = true;
    for (const subscription of this.subscriptions.splice(0)) subscription.dispose();
    this.registrations.length = 0;
    const term = this.term;
    const root = term?.element;
    this.term = null;
    this.fitAddon = null;
    this.webgl = null;
    // Disposing the terminal disposes the addons loaded into it.
    term?.dispose();
    root?.remove();
  }

  /** WebGL contexts can be lost (GPU reset, too many contexts); the DOM renderer takes over. */
  private fallBackToDom(): void {
    this.webgl?.dispose();
    this.webgl = null;
    this.nameRenderer("dom");
  }

  private nameRenderer(renderer: Renderer): void {
    this.term?.element?.setAttribute("data-terminal-renderer", renderer);
  }

  /** What the engine renders the cell grid into: xterm's screen element, sized to exactly the grid. */
  get screenElement(): HTMLElement | null {
    return this.term?.element?.querySelector<HTMLElement>(".xterm-screen") ?? null;
  }

  get inputElement(): HTMLTextAreaElement | null {
    return this.term?.textarea ?? null;
  }

  write(data: string | Uint8Array): void {
    this.term?.write(data);
  }

  paste(text: string): void {
    this.term?.paste(text);
  }

  clear(): void {
    this.term?.clear();
  }

  focus(): void {
    this.term?.focus();
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
    const proposed = this.fitAddon?.proposeDimensions();
    if (!proposed || !Number.isFinite(proposed.cols) || !Number.isFinite(proposed.rows)) return null;
    return { cols: proposed.cols, rows: proposed.rows };
  }

  setFontSize(px: number): void {
    if (this.term) this.term.options.fontSize = px;
  }

  fontSize(): number {
    return this.term?.options.fontSize ?? this.options.fontSize;
  }

  scrollbackLength(): number {
    return this.term?.buffer.active.baseY ?? 0;
  }

  viewportTop(): number {
    return this.term?.buffer.active.viewportY ?? 0;
  }

  scrollToLine(line: number): void {
    const term = this.term;
    if (!term) return;
    term.scrollToLine(Math.max(0, Math.min(term.buffer.active.baseY, Math.floor(line))));
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
    return (this.term?.modes.mouseTrackingMode ?? "none") !== "none";
  }

  alternateScreen(): boolean {
    return this.term?.buffer.active.type === "alternate";
  }

  readLines(): EngineBufferLine[] {
    const buffer = this.term?.buffer.active;
    if (!buffer) return [];
    const lines: EngineBufferLine[] = [];
    const cell = buffer.getNullCell();
    for (let y = 0; y < buffer.length; y++) {
      const line = buffer.getLine(y);
      if (!line) continue;
      let hasInverse = false;
      for (let x = 0; x < line.length; x++) {
        if (line.getCell(x, cell)?.isInverse()) {
          hasInverse = true;
          break;
        }
      }
      // Spaces the program wrote are real cells, which `translateToString(true)` keeps.
      lines.push({ text: line.translateToString(true).trimEnd(), hasInverse });
    }
    return lines;
  }

  cellInverse(line: number, col: number): boolean {
    return Boolean(this.term?.buffer.active.getLine(line)?.getCell(col)?.isInverse());
  }

  onData(listener: (data: string) => void): EngineDisposable {
    return this.listen((term) => term.onData(listener));
  }

  onResize(listener: (size: TerminalEngineSize) => void): EngineDisposable {
    return this.listen((term) => term.onResize(({ cols, rows }) => listener({ cols, rows })));
  }

  onBell(listener: () => void): EngineDisposable {
    return this.listen((term) => term.onBell(listener));
  }

  onTitleChange(listener: (title: string) => void): EngineDisposable {
    return this.listen((term) => term.onTitleChange(listener));
  }

  onScroll(listener: (viewportTop: number) => void): EngineDisposable {
    return this.listen((term) => term.onScroll(listener));
  }

  onBufferChange(listener: (alternate: boolean) => void): EngineDisposable {
    return this.listen((term) => term.buffer.onBufferChange((buffer) => listener(buffer.type === "alternate")));
  }

  onWriteParsed(listener: () => void): EngineDisposable {
    return this.listen((term) => term.onWriteParsed(listener));
  }

  /** Registers on the terminal now when it exists, otherwise as soon as `open` creates it. */
  private listen(register: (term: Terminal) => EngineDisposable): EngineDisposable {
    if (this.term) {
      const subscription = register(this.term);
      this.subscriptions.push(subscription);
      return subscription;
    }
    let subscription: EngineDisposable | null = null;
    const registration = (term: Terminal): EngineDisposable => {
      subscription = register(term);
      return subscription;
    };
    this.registrations.push(registration);
    return {
      dispose: () => {
        const index = this.registrations.indexOf(registration);
        if (index >= 0) this.registrations.splice(index, 1);
        subscription?.dispose();
      },
    };
  }
}

/**
 * An xterm.js engine whose WebGL renderer comes from `createWebgl` — the seam that lets a spec
 * stand in for the GPU (headless browsers give no WebGL context) and lose the context on demand.
 */
export function createXtermEngine(
  options: TerminalEngineOptions,
  createWebgl: WebglRendererFactory = createWebglAddon,
): TerminalEngine {
  return new XtermEngine(options, createWebgl);
}

export const xtermEngine: TerminalEngineFactory = (options: TerminalEngineOptions): TerminalEngine =>
  createXtermEngine(options);
