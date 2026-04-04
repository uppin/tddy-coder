import React from "react";
import { GhosttyTerminal } from "../../src/components/GhosttyTerminal";
import { GhosttyTerminalLiveKit } from "../../src/components/GhosttyTerminalLiveKit";
import { ConnectionTerminalChrome } from "../../src/components/connection/ConnectionTerminalChrome";

/**
 * Acceptance contract (embedded terminal zoom / pitch):
 * - Zoom controls share the terminal surface (see ConnectionTerminalChrome or sibling layout).
 * - data-testid terminal-zoom-pitch-in | terminal-zoom-pitch-out | terminal-zoom-reset
 * - ghostty-terminal exposes data-terminal-font-size (integer string) for assertions
 * - At bounds, pitch-in / pitch-out buttons are disabled per min/max font (defaults 8–32 if not otherwise exposed)
 */

const VIEWPORT_STYLE: React.CSSProperties = {
  width: 480,
  height: 320,
  position: "relative",
};

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
    const onResize = cy.stub().as("onResize");
    const onDisconnect = cy.stub();
    cy.mount(
      <div style={VIEWPORT_STYLE}>
        <GhosttyTerminal fontSize={14} onResize={onResize} />
        <ConnectionTerminalChrome overlayStatus="connected" onDisconnect={onDisconnect} />
      </div>
    );
    cy.get("[data-testid='ghostty-terminal']", { timeout: 10000 }).should("exist");
    cy.get("@onResize", { timeout: 8000 }).should("have.been.called");
    cy.get("@onResize").then((stub) => {
      const s = stub as unknown as { getCalls: () => { args: unknown[] }[] };
      const first = s.getCalls()[0].args[0] as { cols: number; rows: number };
      expect(first.cols, "initial cols").to.be.greaterThan(0);
      expect(first.rows, "initial rows").to.be.greaterThan(0);
      cy.wrap(first.cols * first.rows).as("initialCells");
    });
    cy.get("[data-testid='terminal-zoom-pitch-in']").should("be.visible").click();
    cy.get("[data-testid='ghostty-terminal']").should("have.attr", "data-terminal-font-size");
    cy.get("[data-testid='ghostty-terminal']").then(($el) => {
      const fs = Number($el.attr("data-terminal-font-size"));
      expect(fs, "font size after pitch-in").to.be.greaterThan(14);
    });
    cy.get("@onResize").should((stub) => {
      const s = stub as unknown as { getCalls: () => { args: unknown[] }[] };
      const calls = s.getCalls();
      const last = calls[calls.length - 1].args[0] as { cols: number; rows: number };
      expect(last.cols).to.be.greaterThan(0);
      expect(last.rows).to.be.greaterThan(0);
    });
    cy.get("@initialCells").then((initialCells) => {
      cy.get("@onResize").then((stub) => {
        const s = stub as unknown as { getCalls: () => { args: unknown[] }[] };
        const last = s.getCalls()[s.getCalls().length - 1].args[0] as {
          cols: number;
          rows: number;
        };
        expect(last.cols * last.rows).to.be.at.most(
          Number(initialCells),
          "larger font in fixed viewport should not increase cell count vs baseline"
        );
      });
    });
  });

  it("terminal_pitch_out_decreases_font_and_fires_resize_with_smaller_dimensions_when_viewport_fixed", () => {
    const onResize = cy.stub().as("onResize");
    const onDisconnect = cy.stub();
    cy.mount(
      <div style={VIEWPORT_STYLE}>
        <GhosttyTerminal fontSize={14} onResize={onResize} />
        <ConnectionTerminalChrome overlayStatus="connected" onDisconnect={onDisconnect} />
      </div>
    );
    cy.get("[data-testid='ghostty-terminal']", { timeout: 10000 }).should("exist");
    cy.get("@onResize", { timeout: 8000 }).should("have.been.called");
    cy.get("@onResize").then((stub) => {
      const s = stub as unknown as { getCalls: () => { args: unknown[] }[] };
      const first = s.getCalls()[0].args[0] as { cols: number; rows: number };
      cy.wrap(first.cols * first.rows).as("initialCells");
    });
    cy.get("[data-testid='terminal-zoom-pitch-out']").should("be.visible").click();
    cy.get("[data-testid='ghostty-terminal']").should("have.attr", "data-terminal-font-size");
    cy.get("[data-testid='ghostty-terminal']").then(($el) => {
      const fs = Number($el.attr("data-terminal-font-size"));
      expect(fs, "font size after pitch-out").to.be.lessThan(14);
    });
    cy.get("@initialCells").then((initialCells) => {
      cy.get("@onResize").then((stub) => {
        const s = stub as unknown as { getCalls: () => { args: unknown[] }[] };
        const last = s.getCalls()[s.getCalls().length - 1].args[0] as {
          cols: number;
          rows: number;
        };
        expect(last.cols * last.rows).to.be.at.least(
          Number(initialCells),
          "smaller font in fixed viewport should not shrink cell count vs baseline"
        );
      });
    });
  });

  it("terminal_zoom_reset_restores_baseline_font_and_matching_resize", () => {
    const onResize = cy.stub().as("onResize");
    const onDisconnect = cy.stub();
    cy.mount(
      <div style={VIEWPORT_STYLE}>
        <GhosttyTerminal fontSize={14} onResize={onResize} />
        <ConnectionTerminalChrome overlayStatus="connected" onDisconnect={onDisconnect} />
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
    cy.get("[data-testid='terminal-zoom-pitch-in']").click();
    cy.get("[data-testid='terminal-zoom-pitch-in']").click();
    cy.get("[data-testid='ghostty-terminal']").then(($el) => {
      const fs = Number($el.attr("data-terminal-font-size"));
      expect(fs, "font must change before reset").to.be.greaterThan(14);
    });
    cy.get("[data-testid='terminal-zoom-reset']").should("be.visible").click();
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
    const onDisconnect = cy.stub();
    cy.mount(
      <div style={VIEWPORT_STYLE}>
        <GhosttyTerminal fontSize={14} />
        <ConnectionTerminalChrome overlayStatus="connected" onDisconnect={onDisconnect} />
      </div>
    );
    cy.get("[data-testid='ghostty-terminal']", { timeout: 10000 }).should("exist");
    cy.get("[data-testid='terminal-zoom-pitch-in']").should("exist");
    Cypress._.times(45, () => {
      cy.get("[data-testid='terminal-zoom-pitch-in']").then(($btn) => {
        if (!$btn.is(":disabled")) {
          cy.wrap($btn).click();
        }
      });
    });
    cy.get("[data-testid='terminal-zoom-pitch-in']").should("be.disabled");
    cy.get("[data-testid='ghostty-terminal']").should("have.attr", "data-terminal-font-size", "32");
    Cypress._.times(45, () => {
      cy.get("[data-testid='terminal-zoom-pitch-out']").then(($btn) => {
        if (!$btn.is(":disabled")) {
          cy.wrap($btn).click();
        }
      });
    });
    cy.get("[data-testid='terminal-zoom-pitch-out']").should("be.disabled");
    cy.get("[data-testid='ghostty-terminal']").should("have.attr", "data-terminal-font-size", "8");
  });

  it("livekit_resize_osc_enqueued_when_dimensions_change_after_zoom", () => {
    const getToken = () =>
      Promise.resolve({ token: "fake-token", ttlSeconds: BigInt(600) });
    cy.window().then((win) => {
      const orig = win.console.log.bind(win.console);
      cy.stub(win.console, "log").callsFake((...args: unknown[]) => {
        orig(...args);
      });
    });
    cy.mount(
      <div style={{ height: 400, position: "relative" }}>
        <GhosttyTerminalLiveKit
          url="ws://localhost:9999"
          token="fake-token"
          getToken={getToken}
          ttlSeconds={BigInt(600)}
        />
      </div>
    );
    cy.get("[data-testid='ghostty-terminal']", { timeout: 10000 }).should("exist");
    cy.wait(500);
    cy.window().then((win) => {
      const stub = win.console.log as unknown as {
        getCalls: () => { args: unknown[] }[];
      };
      cy.wrap(countResizeOscCalls(stub)).as("resizeOscBefore");
    });
    cy.get("[data-testid='terminal-zoom-pitch-in']").should("be.visible").click();
    cy.get("[data-testid='ghostty-terminal']").then(($el) => {
      const fs = Number($el.attr("data-terminal-font-size"));
      expect(fs, "livekit path: font must increase after pitch-in").to.be.greaterThan(14);
    });
    cy.wait(300);
    cy.window().then((win) => {
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
    cy.window().then((win) => {
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
