/**
 * Cypress component tests: SessionTrafficStrip
 *
 * Changeset: `byte-traffic`
 * PRD: `docs/ft/web/session-drawer.md` (Session Traffic Strip)
 *
 * Tests the presentational strip component in isolation with direct props.
 * Covers: initial zero state, humanized byte/rate display, ping display,
 * null ping placeholder, and prop updates.
 *
 * ⚠️ These tests fail until:
 *   1. `SessionTrafficStrip.tsx` is created.
 *   2. `formatTraffic.ts` helpers are implemented.
 */

import React from "react";
import { byTestId, TEST_IDS } from "../support/testIds";

// This import fails until SessionTrafficStrip.tsx is created.
import { SessionTrafficStrip } from "../../src/components/sessions/SessionTrafficStrip";

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

function mountStrip(props: {
  bytesIn?: number;
  bytesOut?: number;
  inRate?: number;
  outRate?: number;
  pingMs?: number | null;
}) {
  cy.mount(
    <SessionTrafficStrip
      bytesIn={props.bytesIn ?? 0}
      bytesOut={props.bytesOut ?? 0}
      inRate={props.inRate ?? 0}
      outRate={props.outRate ?? 0}
      pingMs={props.pingMs ?? null}
    />,
  );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("SessionTrafficStrip — component (Cypress)", () => {
  // -------------------------------------------------------------------------
  // AC1: Strip renders with the correct testid
  // -------------------------------------------------------------------------

  it("renders the strip element with data-testid='session-traffic-strip'", () => {
    mountStrip({});
    byTestId(TEST_IDS.sessionTrafficStrip).should("exist");
  });

  // -------------------------------------------------------------------------
  // AC2: Zero state displays 0 B and — ping
  // -------------------------------------------------------------------------

  it("shows '0 B' for both in and out totals when all bytes are zero", () => {
    mountStrip({ bytesIn: 0, bytesOut: 0 });
    byTestId(TEST_IDS.sessionTrafficBytesIn).should("contain.text", "0 B");
    byTestId(TEST_IDS.sessionTrafficBytesOut).should("contain.text", "0 B");
  });

  it("shows '0 B/s' for both rates when all rates are zero", () => {
    mountStrip({ inRate: 0, outRate: 0 });
    byTestId(TEST_IDS.sessionTrafficRateIn).should("contain.text", "0 B/s");
    byTestId(TEST_IDS.sessionTrafficRateOut).should("contain.text", "0 B/s");
  });

  it("shows em-dash placeholder for ping when pingMs is null", () => {
    mountStrip({ pingMs: null });
    byTestId(TEST_IDS.sessionTrafficPing).should("contain.text", "—");
  });

  // -------------------------------------------------------------------------
  // AC3: Byte totals are humanized
  // -------------------------------------------------------------------------

  it("formats bytesIn < 1000 as plain bytes", () => {
    mountStrip({ bytesIn: 512 });
    byTestId(TEST_IDS.sessionTrafficBytesIn).should("contain.text", "512 B");
  });

  it("formats bytesOut >= 1000 as kB", () => {
    mountStrip({ bytesOut: 2048 });
    byTestId(TEST_IDS.sessionTrafficBytesOut).should("contain.text", "kB");
  });

  it("formats large bytesIn as MB", () => {
    mountStrip({ bytesIn: 3_500_000 });
    byTestId(TEST_IDS.sessionTrafficBytesIn).should("contain.text", "MB");
  });

  // -------------------------------------------------------------------------
  // AC4: Rates are humanized with /s suffix
  // -------------------------------------------------------------------------

  it("formats inRate as B/s for small rates", () => {
    mountStrip({ inRate: 200 });
    byTestId(TEST_IDS.sessionTrafficRateIn).should("contain.text", "B/s");
  });

  it("formats outRate as kB/s for rates >= 1000", () => {
    mountStrip({ outRate: 1500 });
    byTestId(TEST_IDS.sessionTrafficRateOut).should("contain.text", "kB/s");
  });

  // -------------------------------------------------------------------------
  // AC5: Ping shows numeric ms value
  // -------------------------------------------------------------------------

  it("shows a numeric ms ping when pingMs is a positive number", () => {
    mountStrip({ pingMs: 42 });
    byTestId(TEST_IDS.sessionTrafficPing).should("contain.text", "42");
    byTestId(TEST_IDS.sessionTrafficPing).should("contain.text", "ms");
  });

  it("shows 0 ms when pingMs is 0", () => {
    mountStrip({ pingMs: 0 });
    byTestId(TEST_IDS.sessionTrafficPing).should("contain.text", "0 ms");
  });

  // -------------------------------------------------------------------------
  // AC6: Strip updates on prop changes
  // -------------------------------------------------------------------------

  it("updates displayed bytes when props change", () => {
    // Given — initial render
    const props = { bytesIn: 100, bytesOut: 200, inRate: 0, outRate: 0, pingMs: null };

    const mountUpdatable = (updatedProps: typeof props) =>
      cy.mount(
        <SessionTrafficStrip {...updatedProps} />,
      );

    mountUpdatable(props);
    byTestId(TEST_IDS.sessionTrafficBytesIn).should("contain.text", "100 B");

    // When — props update
    mountUpdatable({ ...props, bytesIn: 500, bytesOut: 1000 });

    // Then — strip shows new values
    byTestId(TEST_IDS.sessionTrafficBytesIn).should("contain.text", "500 B");
    byTestId(TEST_IDS.sessionTrafficBytesOut).should("contain.text", "1.0 kB");
  });

  it("updates ping display when pingMs changes from null to a value", () => {
    // Given — no ping
    cy.mount(<SessionTrafficStrip bytesIn={0} bytesOut={0} inRate={0} outRate={0} pingMs={null} />);
    byTestId(TEST_IDS.sessionTrafficPing).should("contain.text", "—");

    // When — ping becomes available
    cy.mount(<SessionTrafficStrip bytesIn={0} bytesOut={0} inRate={0} outRate={0} pingMs={23} />);

    // Then
    byTestId(TEST_IDS.sessionTrafficPing).should("contain.text", "23 ms");
  });

  // -------------------------------------------------------------------------
  // Layout: strip is a flex row (thin, horizontal)
  // -------------------------------------------------------------------------

  it("renders as a horizontal flex container (not a block stack)", () => {
    mountStrip({ bytesIn: 100, bytesOut: 200, inRate: 50, outRate: 75, pingMs: 15 });

    // Check Tailwind class rather than computed styles — avoids requiring global CSS in tests.
    byTestId(TEST_IDS.sessionTrafficStrip).should("have.class", "flex");
    byTestId(TEST_IDS.sessionTrafficStrip).should("not.have.class", "flex-col");
  });
});
