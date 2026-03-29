/**
 * E2E: Ghostty + LiveKit + echo_terminal — same segmented payload and contiguous-prefix
 * check as `packages/tddy-e2e/tests/grpc_terminal_rpc.rs` large echo tests.
 *
 * echo_terminal echoes a full line after Enter (not char-by-char), so the payload is typed
 * and submitted with Enter. Assertions use hidden `terminal-buffer-text` (Ghostty buffer text).
 *
 * Requires: LIVEKIT_TESTKIT_WS_URL, `cargo build -p tddy-livekit --example echo_terminal`,
 * Storybook static bundle (`bun run build-storybook`).
 */

const LARGE_ECHO_CHAR_CAP = 1000;
const LARGE_ECHO_SEGMENTS = 10;
const LARGE_ECHO_E2E_ROOM = "large-echo-e2e";

function buildLargeEchoSegmentedPayload(
  totalLen: number,
  numSegments: number
): { full: string; segments: string[] } {
  const headers = Array.from({ length: numSegments }, (_, i) => `#SEG-${i}:`);
  const headerChars = headers.reduce((acc, h) => acc + h.length, 0);
  if (headerChars > totalLen) {
    throw new Error(
      `segment headers exceed totalLen=${totalLen} (headers use ${headerChars} chars, ${numSegments} segments)`
    );
  }
  const bodyTotal = totalLen - headerChars;
  const base = Math.floor(bodyTotal / numSegments);
  const rem = bodyTotal % numSegments;
  const segments: string[] = [];
  for (let i = 0; i < numSegments; i++) {
    const bodyLen = base + (i < rem ? 1 : 0);
    segments.push(headers[i] + "a".repeat(bodyLen));
  }
  const full = segments.join("");
  if (full.length !== totalLen) {
    throw new Error(`expected ${totalLen} chars, got ${full.length}`);
  }
  return { full, segments };
}

function compactNoWs(s: string): string {
  return Array.from(s)
    .filter((c) => !/\s/.test(c))
    .join("");
}

function longestContiguousPrefixLen(compact: string, expectedNoWs: string): number {
  let lo = 0;
  let hi = expectedNoWs.length;
  while (lo < hi) {
    const mid = Math.ceil((lo + hi) / 2);
    if (compact.includes(expectedNoWs.slice(0, mid))) {
      lo = mid;
    } else {
      hi = mid - 1;
    }
  }
  return lo;
}

describe("Ghostty large segmented echo (LiveKit + echo_terminal)", () => {
  let serverUrl: string;
  let clientToken: string;
  let roomName: string;

  before(function () {
    if (!Cypress.env("LIVEKIT_TESTKIT_WS_URL")) {
      this.skip();
      return;
    }
    return cy
      .task("startEchoTerminal", { roomName: LARGE_ECHO_E2E_ROOM })
      .then((result) => {
        const r = result as {
          url: string;
          clientToken: string;
          roomName: string;
        };
        serverUrl = r.url;
        clientToken = r.clientToken;
        roomName = r.roomName;
      });
  });

  after(() => {
    cy.task("stopEchoTerminal");
  });

  it("shows full segmented echo in terminal buffer after line submit (matches Rust oracle)", () => {
    cy.viewport(1400, 900);

    const featureLen = LARGE_ECHO_CHAR_CAP;
    const { full: expected, segments } = buildLargeEchoSegmentedPayload(
      featureLen,
      LARGE_ECHO_SEGMENTS
    );
    const expectedNoWs = compactNoWs(expected);

    const storyUrl =
      `/iframe.html?id=components-ghosttyterminal--live-kit-echo-large-segmented` +
      `&url=${encodeURIComponent(serverUrl)}` +
      `&token=${encodeURIComponent(clientToken)}` +
      `&roomName=${encodeURIComponent(roomName)}`;

    cy.visit(storyUrl);

    cy.get("body", { timeout: 10000 }).should("be.visible");

    cy.get("[data-testid='connection-status-dot']", { timeout: 25000 })
      .should("be.visible")
      .and("have.attr", "data-connection-status", "connected");
    cy.get("[data-testid='livekit-status']").should("not.be.visible");

    cy.get("[data-testid='first-output-received']", { timeout: 15000 }).should(
      "exist"
    );

    cy.get("[data-testid='ghostty-terminal']", { timeout: 10000 })
      .should("exist")
      .click();

    cy.get("[data-testid='ghostty-terminal']").type(expected, {
      delay: 0,
    });
    cy.get("[data-testid='ghostty-terminal']").type("{enter}");

    cy.get("[data-testid='terminal-buffer-text']", { timeout: 120000 }).should(
      ($el) => {
        const raw = $el.text();
        const compact = compactNoWs(raw);
        const lo = longestContiguousPrefixLen(compact, expectedNoWs);
        const segFlags = segments.map((seg) =>
          compact.includes(compactNoWs(seg))
        );
        const markerFlags = segments.map((_, i) =>
          compact.includes(`#SEG-${i}:`)
        );
        expect(
          lo,
          `vt100-style contiguous echo (no ws): longest prefix ${lo} of ${expectedNoWs.length}; per-segment full: ${JSON.stringify(segFlags)}; markers: ${JSON.stringify(markerFlags)}`
        ).to.eq(expectedNoWs.length);
      }
    );
  });
});
