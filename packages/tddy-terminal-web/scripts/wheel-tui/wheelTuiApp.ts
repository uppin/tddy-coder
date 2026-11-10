/**
 * A deliberately tiny full-screen TUI that does one thing: show what mouse and key input reached it.
 *
 * Dev/test tool, not product code. It stands in for the Claude CLI's TUI when diagnosing how
 * tddy-web routes the mouse wheel: the terminal either reports the wheel to the application (SGR
 * button 64 up / 65 down), or turns it into arrow keys, or scrolls its own viewport — and only the
 * first of those is the application's to see. This app counts each, so the three are told apart on
 * its own screen.
 *
 * Pure (bytes in, bytes out) so the same app runs in a Cypress component test, wired to a fake
 * terminal feed, and in a real PTY via `wheel-tui.ts`.
 */

export type MouseTrackingMode = "1000" | "1002" | "1003" | "off";

export interface WheelTuiOptions {
  /** Draw on the alternate screen (DEC 1049) — a full-screen TUI — or on the normal screen. */
  alternateScreen: boolean;
  /** Which DEC mouse-tracking mode to request (always with SGR encoding, 1006), or none. */
  mouse: MouseTrackingMode;
}

const SGR_MOUSE_REPORT = /\x1b\[<(\d+);(\d+);(\d+)([Mm])/g;
const WHEEL_BIT = 64;
const WHEEL_DOWN_BIT = 1;
const ARROW_UP = /\x1b(?:\[|O)A/g;
const ARROW_DOWN = /\x1b(?:\[|O)B/g;

export class WheelTuiApp {
  wheelUp = 0;
  wheelDown = 0;
  arrowUp = 0;
  arrowDown = 0;
  otherInputBytes = 0;
  lastEvent = "none yet";

  constructor(private readonly options: WheelTuiOptions) {}

  /** Mode setup plus the first frame: what the application writes on launch. */
  start(): string {
    const { alternateScreen, mouse } = this.options;
    const modes =
      (alternateScreen ? "\x1b[?1049h" : "") +
      (mouse === "off" ? "" : `\x1b[?${mouse}h\x1b[?1006h`);
    return modes + this.frame();
  }

  /** Terminal input reached the application: count it, repaint. */
  receive(input: string): string {
    let rest = input.replace(SGR_MOUSE_REPORT, (_all, b, col, row, kind) => {
      const button = Number(b);
      if (kind === "M" && button & WHEEL_BIT) {
        const down = (button & WHEEL_DOWN_BIT) !== 0;
        if (down) this.wheelDown += 1;
        else this.wheelUp += 1;
        this.lastEvent = `wheel ${down ? "down" : "up"} (SGR ${button}) at col ${col} row ${row}`;
      } else {
        this.lastEvent = `mouse (SGR ${button}${kind}) at col ${col} row ${row}`;
      }
      return "";
    });
    rest = rest.replace(ARROW_UP, () => {
      this.arrowUp += 1;
      this.lastEvent = "arrow up key";
      return "";
    });
    rest = rest.replace(ARROW_DOWN, () => {
      this.arrowDown += 1;
      this.lastEvent = "arrow down key";
      return "";
    });
    this.otherInputBytes += rest.length;
    return this.frame();
  }

  /** Restore the terminal: what the application writes on exit. */
  stop(): string {
    const { alternateScreen, mouse } = this.options;
    return (
      (mouse === "off" ? "" : "\x1b[?1006l" + `\x1b[?${mouse}l`) + (alternateScreen ? "\x1b[?1049l" : "")
    );
  }

  private frame(): string {
    const { alternateScreen, mouse } = this.options;
    const lines = [
      `WHEEL-TUI  screen=${alternateScreen ? "alternate" : "normal"}  mouse=${mouse === "off" ? "off" : `${mouse}+1006`}`,
      `wheel up:    ${this.wheelUp}`,
      `wheel down:  ${this.wheelDown}`,
      `arrow up:    ${this.arrowUp}   arrow down: ${this.arrowDown}`,
      `last event:  ${this.lastEvent}`,
      "",
      "scroll the wheel over this pane; q or Ctrl+C quits",
    ];
    return "\x1b[H\x1b[2J" + lines.join("\r\n");
  }
}
