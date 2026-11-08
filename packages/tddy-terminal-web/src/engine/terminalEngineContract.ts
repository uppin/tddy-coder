/**
 * The behaviour every `TerminalEngine` must show, as a reusable Cypress suite.
 *
 * Run it once per engine from a component spec (`describeTerminalEngine("ghosttyEngine",
 * ghosttyEngine)`): engines render into a real DOM and some load WASM, so the suite needs a browser.
 * Every scroll position asserted here counts from the top of the buffer.
 */

import type {
  EngineDisposable,
  TerminalEngine,
  TerminalEngineFactory,
  TerminalEngineOptions,
  TerminalEngineSize,
} from "./TerminalEngine";

const SCREEN_ROWS = 10;
const SCREEN_COLS = 80;

const DEFAULT_OPTIONS: TerminalEngineOptions = {
  cols: SCREEN_COLS,
  rows: SCREEN_ROWS,
  fontSize: 14,
  scrollback: 1000,
};

/** `count` numbered lines (`line 0` … `line <count-1>`), each ending in CRLF. */
function numberedLines(count: number): string {
  return Array.from({ length: count }, (_, i) => `line ${i}\r\n`).join("");
}

/** Records every value a listener receives, for later assertions. */
class Recorder<T> {
  readonly values: T[] = [];
  readonly listener = (value: T): void => {
    this.values.push(value);
  };
}

/** An engine opened into a fixed-size container attached to the test document. */
class OpenEngine {
  private readonly subscriptions: EngineDisposable[] = [];
  readonly data = new Recorder<string>();
  readonly resizes = new Recorder<TerminalEngineSize>();
  readonly bells = new Recorder<void>();
  readonly titles = new Recorder<string>();
  readonly scrolls = new Recorder<number>();
  readonly bufferChanges = new Recorder<boolean>();

  constructor(
    readonly engine: TerminalEngine,
    readonly container: HTMLElement,
  ) {
    this.subscriptions.push(
      engine.onData(this.data.listener),
      engine.onResize(this.resizes.listener),
      engine.onBell(() => this.bells.listener(undefined)),
      engine.onTitleChange(this.titles.listener),
      engine.onScroll(this.scrolls.listener),
      engine.onBufferChange(this.bufferChanges.listener),
    );
  }

  /** Writes and yields once the engine has taken the output in (its buffer shows `untilText`). */
  write(data: string, untilText?: string): Cypress.Chainable<OpenEngine> {
    cy.then(() => this.engine.write(data));
    if (untilText !== undefined) {
      cy.wrap<OpenEngine>(this, { log: false }).should((open) => {
        expect(open.lineTexts().join("\n")).to.contain(untilText);
      });
    }
    return cy.wrap<OpenEngine>(this, { log: false });
  }

  /** Writes `count` numbered lines and waits for the last one to land. */
  writeNumberedLines(count: number): Cypress.Chainable<OpenEngine> {
    return this.write(numberedLines(count), `line ${count - 1}`);
  }

  lineTexts(): string[] {
    return this.engine.readLines().map((line) => line.text);
  }

  dispose(): void {
    for (const subscription of this.subscriptions) subscription.dispose();
    this.engine.dispose();
    this.container.remove();
  }
}

function anOpenEngine(
  makeEngine: TerminalEngineFactory,
  overrides: Partial<TerminalEngineOptions> = {},
): Cypress.Chainable<OpenEngine> {
  return cy.document().then((doc) => {
    const container = doc.createElement("div");
    container.style.width = "800px";
    container.style.height = "240px";
    doc.body.appendChild(container);
    const engine = makeEngine({ ...DEFAULT_OPTIONS, ...overrides });
    return cy.wrap(engine.open(container), { log: false }).then(() => new OpenEngine(engine, container));
  });
}

/** Asserts on the engine until the expectation holds (engines may apply writes asynchronously). */
function eventually(open: OpenEngine, expectation: (open: OpenEngine) => void): void {
  cy.wrap(open, { log: false }).should(expectation);
}

export function describeTerminalEngine(name: string, makeEngine: TerminalEngineFactory): void {
  describe(`${name} — TerminalEngine contract`, () => {
    let current: OpenEngine | null = null;
    const opened = (overrides: Partial<TerminalEngineOptions> = {}) =>
      anOpenEngine(makeEngine, overrides).then((open) => {
        current = open;
        return open;
      });

    afterEach(() => {
      current?.dispose();
      current = null;
    });

    describe("output", () => {
      it("reads written lines back in order, trailing blanks trimmed", () => {
        // Given
        opened().then((open) => {
          // When
          open.write("alpha   \r\nbeta\r\n", "beta");

          // Then
          eventually(open, (o) => {
            expect(o.lineTexts().slice(0, 2)).to.deep.equal(["alpha", "beta"]);
          });
        });
      });

      it("clear removes earlier output from the buffer", () => {
        // Given
        opened().then((open) => {
          open.write("forget me\r\n", "forget me");

          // When
          cy.then(() => open.engine.clear());

          // Then
          eventually(open, (o) => {
            expect(o.lineTexts().join("\n")).not.to.contain("forget me");
          });
        });
      });

      it("reports cells drawn in inverse video", () => {
        // Given
        opened().then((open) => {
          // When
          open.write("\x1b[7mX\x1b[0mY\r\n", "XY");

          // Then
          eventually(open, (o) => {
            expect(o.engine.cellInverse(0, 0), "inverse cell").to.equal(true);
            expect(o.engine.cellInverse(0, 1), "plain cell").to.equal(false);
            expect(o.engine.readLines()[0]).to.deep.equal({ text: "XY", hasInverse: true });
          });
        });
      });
    });

    describe("events", () => {
      it("emits pasted text as input through onData", () => {
        // Given
        opened().then((open) => {
          // When
          cy.then(() => open.engine.paste("hello"));

          // Then
          eventually(open, (o) => expect(o.data.values.join("")).to.equal("hello"));
        });
      });

      it("announces a bell and a title change", () => {
        // Given
        opened().then((open) => {
          // When
          open.write("\x07\x1b]0;build log\x07");

          // Then
          eventually(open, (o) => {
            expect(o.bells.values, "bells").to.have.length(1);
            expect(o.titles.values, "titles").to.deep.equal(["build log"]);
          });
        });
      });
    });

    describe("geometry", () => {
      it("starts at the grid it was created with", () => {
        // Given / When
        opened({ cols: 72, rows: 12 }).then((open) => {
          // Then
          expect({ cols: open.engine.cols, rows: open.engine.rows }).to.deep.equal({ cols: 72, rows: 12 });
        });
      });

      it("resizes the grid and reports the new size", () => {
        // Given
        opened().then((open) => {
          // When
          cy.then(() => open.engine.resize(100, 30));

          // Then
          eventually(open, (o) => {
            expect({ cols: o.engine.cols, rows: o.engine.rows }).to.deep.equal({ cols: 100, rows: 30 });
            expect(o.resizes.values).to.deep.include({ cols: 100, rows: 30 });
          });
        });
      });

      it("proposes a grid that fits its container and changes it with the font size", () => {
        // Given
        opened({ fontSize: 14 }).then((open) => {
          const atFourteen = open.engine.proposeDimensions();

          // When
          cy.then(() => open.engine.setFontSize(28));

          // Then
          eventually(open, (o) => {
            const atTwentyEight = o.engine.proposeDimensions();
            expect(atFourteen, "proposal at 14px").to.not.equal(null);
            expect(atTwentyEight, "proposal at 28px").to.not.equal(null);
            expect(o.engine.fontSize()).to.equal(28);
            expect(atTwentyEight!.cols).to.be.lessThan(atFourteen!.cols);
            expect(atTwentyEight!.rows).to.be.lessThan(atFourteen!.rows);
          });
        });
      });

      it("renders into a screen element inside its container", () => {
        // Given / When
        opened().then((open) => {
          // Then
          expect(open.engine.screenElement, "screen element").to.not.equal(null);
          expect(open.container.contains(open.engine.screenElement!)).to.equal(true);
        });
      });
    });

    describe("top-based scrolling", () => {
      it("has no scrollback and its viewport on line 0 while output fits the screen", () => {
        // Given
        opened().then((open) => {
          // When
          open.writeNumberedLines(3);

          // Then
          eventually(open, (o) => {
            expect(o.engine.scrollbackLength(), "scrollback").to.equal(0);
            expect(o.engine.viewportTop(), "viewport top").to.equal(0);
          });
        });
      });

      it("follows the live tip as output scrolls off: viewportTop equals the scrollback length", () => {
        // Given
        opened().then((open) => {
          // When
          open.writeNumberedLines(50);

          // Then
          eventually(open, (o) => {
            expect(o.engine.scrollbackLength(), "scrollback").to.be.greaterThan(0);
            expect(o.engine.viewportTop(), "viewport top").to.equal(o.engine.scrollbackLength());
          });
        });
      });

      it("scrollToTop shows line 0 and reports it through onScroll", () => {
        // Given
        opened().then((open) => {
          open.writeNumberedLines(50);

          // When
          cy.then(() => open.engine.scrollToTop());

          // Then
          eventually(open, (o) => {
            expect(o.engine.viewportTop(), "viewport top").to.equal(0);
            expect(o.scrolls.values.at(-1), "last onScroll").to.equal(0);
            expect(o.lineTexts()[o.engine.viewportTop()]).to.equal("line 0");
          });
        });
      });

      it("scrollToLine puts the given top-based line on the viewport's first row", () => {
        // Given
        opened().then((open) => {
          open.writeNumberedLines(50);

          // When
          cy.then(() => open.engine.scrollToLine(7));

          // Then
          eventually(open, (o) => {
            expect(o.engine.viewportTop(), "viewport top").to.equal(7);
            expect(o.lineTexts()[o.engine.viewportTop()]).to.equal("line 7");
            expect(o.scrolls.values.at(-1), "last onScroll").to.equal(7);
          });
        });
      });

      it("scrollToLine past the end clamps to the live tip", () => {
        // Given
        opened().then((open) => {
          open.writeNumberedLines(50);
          cy.then(() => open.engine.scrollToTop());

          // When
          cy.then(() => open.engine.scrollToLine(10_000));

          // Then
          eventually(open, (o) => expect(o.engine.viewportTop()).to.equal(o.engine.scrollbackLength()));
        });
      });

      it("scrollLines moves relative to the viewport, negative into the scrollback", () => {
        // Given
        opened().then((open) => {
          open.writeNumberedLines(50);

          // When
          cy.then(() => open.engine.scrollLines(-3));

          // Then
          eventually(open, (o) => expect(o.engine.viewportTop()).to.equal(o.engine.scrollbackLength() - 3));
        });
      });

      it("scrollToBottom returns from the scrollback to the live tip", () => {
        // Given
        opened().then((open) => {
          open.writeNumberedLines(50);
          cy.then(() => open.engine.scrollToTop());

          // When
          cy.then(() => open.engine.scrollToBottom());

          // Then
          eventually(open, (o) => {
            expect(o.engine.viewportTop(), "viewport top").to.equal(o.engine.scrollbackLength());
            expect(o.scrolls.values.at(-1), "last onScroll").to.equal(o.engine.scrollbackLength());
          });
        });
      });
    });

    describe("modes", () => {
      it("tracks the alternate screen and announces each switch", () => {
        // Given
        opened().then((open) => {
          // When
          open.write("\x1b[?1049h");

          // Then
          eventually(open, (o) => {
            expect(o.engine.alternateScreen(), "on the alternate screen").to.equal(true);
            expect(o.bufferChanges.values).to.deep.equal([true]);
          });

          // When
          open.write("\x1b[?1049l");

          // Then
          eventually(open, (o) => {
            expect(o.engine.alternateScreen(), "back on the normal screen").to.equal(false);
            expect(o.bufferChanges.values).to.deep.equal([true, false]);
          });
        });
      });

      it("tracks whether the running program asked for mouse reports", () => {
        // Given
        opened().then((open) => {
          expect(open.engine.mouseTracking(), "before any request").to.equal(false);

          // When
          open.write("\x1b[?1000h\x1b[?1006h");

          // Then
          eventually(open, (o) => expect(o.engine.mouseTracking(), "after enabling").to.equal(true));

          // When
          open.write("\x1b[?1000l\x1b[?1006l");

          // Then
          eventually(open, (o) => expect(o.engine.mouseTracking(), "after disabling").to.equal(false));
        });
      });
    });

    describe("lifecycle", () => {
      it("dispose detaches the engine's screen from the document", () => {
        // Given
        opened().then((open) => {
          const screen = open.engine.screenElement;

          // When
          cy.then(() => {
            open.engine.dispose();
            current = null;
          });

          // Then
          cy.then(() => {
            expect(screen?.isConnected ?? false, "screen still attached").to.equal(false);
            open.container.remove();
          });
        });
      });
    });
  });
}
