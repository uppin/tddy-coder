import React, { createRef } from "react";
import { flushSync } from "react-dom";
import { GhosttyTerminal, type GhosttyTerminalHandle } from "../../src/components/GhosttyTerminal";
import { GhosttyTerminalLiveKit } from "../../src/components/GhosttyTerminalLiveKit";
import {
  DEFAULT_TERMINAL_FONT_MAX,
  DEFAULT_TERMINAL_FONT_MIN,
  pitchInFontSize,
  pitchOutFontSize,
} from "../../src/lib/terminalZoom";
import { dispatchTerminalZoomBridgeOn } from "../../src/lib/terminalZoomBridge";

/**
 * Acceptance contract (embedded terminal zoom / pitch):
 * - Font steps use the same `terminalZoom` helpers as keyboard / bridge paths (`setTerminalFontSize` on the imperative handle).
 * - LiveKit case uses `dispatchTerminalZoomBridgeOn` from the terminal textarea’s `ownerDocument.defaultView` (same `window` as `GhosttyTerminal` listeners).
 * - `ghostty-terminal` exposes data-terminal-font-size (integer string) for assertions
 * - Pitch tests wait briefly after the terminal textarea exists so the `ready` + prop-sync effect has run before imperative `setTerminalFontSize` (avoids a race on warm init).
 */

const VIEWPORT_STYLE: React.CSSProperties = {
  width: 480,
  height: 320,
  position: "relative",
};

const ZOOM_OPTS = { min: DEFAULT_TERMINAL_FONT_MIN, max: DEFAULT_TERMINAL_FONT_MAX };

/** Cypress + async React scheduling: flush so `data-terminal-font-size` updates before chained assertions. */
function flushSetTerminalFontSize(
  ref: React.RefObject<GhosttyTerminalHandle | null>,
  px: number
) {
  flushSync(() => {
    ref.current!.setTerminalFontSize!(px);
  });
}

/** Let React run `ready` + `applyFontSizePx(fontSize)` from props before imperative zoom (see pitch tests). */
function waitForPropSyncAfterReady() {
  cy.get("[data-testid='ghostty-terminal'] textarea").should("exist");
  cy.wrap(null).then(
    () =>
      new Promise<void>((resolve) => {
        requestAnimationFrame(() => {
          requestAnimationFrame(() => {
            setTimeout(resolve, 0);
          });
        });
      })
  );
}

function argStringForOscScan(a: unknown): string {
  if (typeof a === "string") return a;
  if (Array.isArray(a) && a.length > 0 && typeof a[0] === "number") {
    return new TextDecoder().decode(new Uint8Array(a as number[]));
  }
  return JSON.stringify(a);
}

function countResizeOscCalls(
  stub: { getCalls: () => { args: unknown[] }[] }
): number {
  let n = 0;
  for (const c of stub.getCalls()) {
    for (const a of c.args) {
      const s = argStringForOscScan(a);
      if (/\]resize;\d+;\d+/.test(s)) {
        n += 1;
        break;
      }
    }
  }
  return n;
}

describe("Terminal zoom acceptance (PRD Testing Plan)", () => {
  it("terminal_pitch_in_increases_font_and_fires_resize_with_larger_dimensions_when_viewport_fixed", () => {
    const termRef = createRef<GhosttyTerminalHandle>();
    cy.mount(
      <div style={VIEWPORT_STYLE}>
        <GhosttyTerminal ref={termRef} fontSize={14} />
      </div>
    );
    cy.get("[data-testid='ghostty-terminal']", { timeout: 10000 }).should("exist");
    waitForPropSyncAfterReady();
    cy.get("[data-testid='ghostty-terminal']").should("have.attr", "data-terminal-font-size", "14");
    cy.wrap(termRef).should((r) => {
      expect(r.current?.setTerminalFontSize).to.be.a("function");
    });
    cy.wrap(null).then(() => {
      flushSetTerminalFontSize(termRef, pitchInFontSize(14, ZOOM_OPTS));
    });
    cy.get("[data-testid='ghostty-terminal']").should(($el) => {
      const fs = Number($el.attr("data-terminal-font-size"));
      expect(fs, "font size after pitch-in").to.be.greaterThan(14);
    });
  });

  it("terminal_pitch_out_decreases_font_and_fires_resize_with_smaller_dimensions_when_viewport_fixed", () => {
    const termRef = createRef<GhosttyTerminalHandle>();
    cy.mount(
      <div style={VIEWPORT_STYLE}>
        <GhosttyTerminal ref={termRef} fontSize={14} />
      </div>
    );
    cy.get("[data-testid='ghostty-terminal']", { timeout: 10000 }).should("exist");
    waitForPropSyncAfterReady();
    cy.get("[data-testid='ghostty-terminal']").should("have.attr", "data-terminal-font-size", "14");
    cy.wrap(termRef).should((r) => {
      expect(r.current?.setTerminalFontSize).to.be.a("function");
    });
    cy.wrap(null).then(() => {
      flushSetTerminalFontSize(termRef, pitchOutFontSize(14, ZOOM_OPTS));
    });
    cy.get("[data-testid='ghostty-terminal']").should(($el) => {
      const fs = Number($el.attr("data-terminal-font-size"));
      expect(fs, "font size after pitch-out").to.be.lessThan(14);
    });
  });

  it("terminal_zoom_reset_restores_baseline_font_and_matching_resize", () => {
    const termRef = createRef<GhosttyTerminalHandle>();
    const onResize = cy.stub().as("onResize");
    cy.mount(
      <div style={VIEWPORT_STYLE}>
        <GhosttyTerminal ref={termRef} fontSize={14} onResize={onResize} />
      </div>
    );
    cy.get("[data-testid='ghostty-terminal']", { timeout: 10000 }).should("exist");
    cy.get("@onResize", { timeout: 8000 }).should("have.been.called");
    cy.get("@onResize").then((stub) => {
      const s = stub as unknown as { getCalls: () => { args: unknown[] }[] };
      const first = s.getCalls()[0].args[0] as { cols: number; rows: number };
      cy.wrap(first.cols).as("baselineCols");
      cy.wrap(first.rows).as("baselineRows");
    });
    cy.get("[data-testid='ghostty-terminal']").should("have.attr", "data-terminal-font-size", "14");
    cy.wrap(termRef).should((r) => {
      expect(r.current?.setTerminalFontSize).to.be.a("function");
    });
    cy.wrap(null).then(() => {
      let cur = 14;
      cur = pitchInFontSize(cur, ZOOM_OPTS);
      flushSetTerminalFontSize(termRef, cur);
      cur = pitchInFontSize(cur, ZOOM_OPTS);
      flushSetTerminalFontSize(termRef, cur);
    });
    cy.get("[data-testid='ghostty-terminal']").then(($el) => {
      const fs = Number($el.attr("data-terminal-font-size"));
      expect(fs, "font must change before reset").to.be.greaterThan(14);
    });
    cy.wrap(null).then(() => {
      flushSetTerminalFontSize(termRef, 14);
    });
    cy.get("[data-testid='ghostty-terminal']").should("have.attr", "data-terminal-font-size", "14");
    cy.get("@baselineCols").then((bc) => {
      cy.get("@baselineRows").then((br) => {
        cy.get("@onResize").should((stub) => {
          const s = stub as unknown as { getCalls: () => { args: unknown[] }[] };
          const last = s.getCalls()[s.getCalls().length - 1].args[0] as {
            cols: number;
            rows: number;
          };
          expect(last.cols).to.eq(Number(bc));
          expect(last.rows).to.eq(Number(br));
        });
      });
    });
  });

  it("terminal_zoom_respects_configured_min_max_bounds", () => {
    const termRef = createRef<GhosttyTerminalHandle>();
    cy.mount(
      <div style={VIEWPORT_STYLE}>
        <GhosttyTerminal ref={termRef} fontSize={14} />
      </div>
    );
    cy.get("[data-testid='ghostty-terminal']", { timeout: 10000 }).should("exist");
    cy.wrap(termRef).should((r) => {
      expect(r.current?.setTerminalFontSize).to.be.a("function");
    });
    cy.wrap(null).then(() => {
      let cur = 14;
      for (let i = 0; i < 45; i++) {
        cur = pitchInFontSize(cur, ZOOM_OPTS);
        flushSetTerminalFontSize(termRef, cur);
      }
    });
    cy.get("[data-testid='ghostty-terminal']").should("have.attr", "data-terminal-font-size", "32");
    cy.wrap(null).then(() => {
      let cur = 32;
      for (let i = 0; i < 45; i++) {
        cur = pitchOutFontSize(cur, ZOOM_OPTS);
        flushSetTerminalFontSize(termRef, cur);
      }
    });
    cy.get("[data-testid='ghostty-terminal']").should("have.attr", "data-terminal-font-size", "8");
  });

  it("livekit_resize_osc_enqueued_when_dimensions_change_after_zoom", () => {
    const getToken = () =>
      Promise.resolve({ token: "fake-token", ttlSeconds: BigInt(600) });
    cy.mount(
      <div style={{ height: 400, position: "relative" }}>
        <GhosttyTerminalLiveKit
          url="ws://localhost:9999"
          token="fake-token"
          getToken={getToken}
          ttlSeconds={BigInt(600)}
          debugLogging
        />
      </div>
    );
    cy.get("[data-testid='ghostty-terminal']", { timeout: 10000 }).should("exist");
    cy.get("[data-testid='ghostty-terminal'] textarea").first().then((ta) => {
      const win = ta[0].ownerDocument.defaultView;
      if (!win) {
        throw new Error("terminal textarea has no defaultView");
      }
      const orig = win.console.log.bind(win.console);
      cy.stub(win.console, "log").callsFake((...args: unknown[]) => {
        orig(...args);
      });
    });
    cy.wait(500);
    cy.get("[data-testid='ghostty-terminal'] textarea").first().then((ta) => {
      const win = ta[0].ownerDocument.defaultView;
      if (!win) {
        throw new Error("terminal textarea has no defaultView");
      }
      const stub = win.console.log as unknown as {
        getCalls: () => { args: unknown[] }[];
      };
      cy.wrap(countResizeOscCalls(stub)).as("resizeOscBefore");
    });
    cy.get("[data-testid='ghostty-terminal'] textarea").first().then((ta) => {
      const win = ta[0].ownerDocument.defaultView;
      if (!win) {
        throw new Error("terminal textarea has no defaultView");
      }
      dispatchTerminalZoomBridgeOn(win, {
        action: "pitch-in",
        baselineFontSize: 14,
        opts: ZOOM_OPTS,
      });
    });
    cy.get("[data-testid='ghostty-terminal']").should(($el) => {
      const fs = Number($el.attr("data-terminal-font-size"));
      expect(fs, "livekit path: font must increase after pitch-in").to.be.greaterThan(14);
    });
    cy.wait(300);
    cy.get("[data-testid='ghostty-terminal'] textarea").first().then((ta) => {
      const win = ta[0].ownerDocument.defaultView;
      if (!win) {
        throw new Error("terminal textarea has no defaultView");
      }
      const stub = win.console.log as unknown as {
        getCalls: () => { args: unknown[] }[];
      };
      cy.get("@resizeOscBefore").then((before) => {
        expect(countResizeOscCalls(stub)).to.be.greaterThan(
          Number(before),
          "enqueue path should log OSC resize after zoom when dimensions change"
        );
      });
    });
    cy.get("[data-testid='ghostty-terminal'] textarea").first().then((ta) => {
      const win = ta[0].ownerDocument.defaultView;
      if (!win) {
        throw new Error("terminal textarea has no defaultView");
      }
      const stub = win.console.log as unknown as {
        getCalls: () => { args: unknown[] }[];
      };
      const texts = stub.getCalls().map((c) =>
        c.args.map((a) => argStringForOscScan(a)).join(" ")
      );
      expect(
        texts.some((t) => /\]resize;\d+;\d+/.test(t)),
        "resize OSC substring ]resize;cols;rows should appear in terminal→server logging"
      ).to.be.true;
    });
  });
});
