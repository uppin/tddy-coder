/**
 * Acceptance: the terminal tracks the display's scale factor.
 *
 * **The load-bearing behavior is a re-render on change, not the initial scale.** ghostty-web 0.4.0
 * reads `devicePixelRatio` once, at renderer construction, and never again — so a terminal that
 * mounted on a 1x external monitor keeps rendering at 1x forever after the window moves to the
 * Retina display, and the compositor upscales it soft. The component re-arms a `matchMedia`
 * resolution query on every change and forces a full re-render, so the canvas backing store must
 * double when the display's scale factor does.
 */

import { aGhosttyTerminal } from "../support/drivers/ghosttyTerminalDriver";

/** The handful of MediaQueryList members the watcher uses. */
interface ControlledQuery {
  listeners: EventListener[];
}

describe("GhosttyTerminal DPI", () => {
  it("rescales the canvas backing store when the display's scale factor changes", () => {
    // Given — a 1x display, and a matchMedia whose resolution queries the test can fire by hand
    // (a real MediaQueryList only fires when the OS actually changes the scale, which a test cannot
    // provoke).
    const resolutionQueries: ControlledQuery[] = [];
    let win: Window | undefined;
    cy.window().then((w) => {
      win = w;
      Object.defineProperty(w, "devicePixelRatio", { value: 1, configurable: true });
      const original = w.matchMedia.bind(w);
      cy.stub(w, "matchMedia").callsFake((query: string) => {
        if (!query.includes("(resolution:")) return original(query);
        const q: ControlledQuery = { listeners: [] };
        resolutionQueries.push(q);
        return {
          matches: true,
          addEventListener: (_t: string, l: EventListener) => q.listeners.push(l),
          removeEventListener: (_t: string, l: EventListener) => {
            q.listeners = q.listeners.filter((x) => x !== l);
          },
        };
      });
    });

    const driver = aGhosttyTerminal({ initialContent: "dpi" })
      .mount()
      .expectExists()
      .expectCanvasExists();

    // Then — the terminal mounted on the 1x display has a 1x backing store
    driver
      .el()
      .find("canvas")
      .then(($canvas) => {
        const widthAt1x = $canvas[0].width;
        expect(widthAt1x).to.be.greaterThan(0);

        // When — the window moves to a 2x display: the OS value changes and the armed
        // query stops matching, which is the only signal the component gets
        Object.defineProperty(win, "devicePixelRatio", { value: 2, configurable: true });
        cy.wrap(null).then(() => {
          for (const listener of [...resolutionQueries[resolutionQueries.length - 1].listeners]) {
            listener(new Event("change"));
          }
        });

        // Then — the backing store re-renders at the new scale
        driver
          .el()
          .find("canvas")
          .should(($c) => {
            const widthAt2x = $c[0].width;
            expect(Math.round(widthAt2x / widthAt1x)).to.eq(
              2,
              `backing store should double on a scale-factor change (${widthAt1x} → ${widthAt2x})`,
            );
          });
      });
  });
});
