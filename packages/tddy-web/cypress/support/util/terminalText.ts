/**
 * Shared terminal text utilities for e2e specs.
 * Extracted from ghostty-terminal.cy.ts and ghostty-large-segmented-echo.cy.ts.
 */

/**
 * Strip ANSI escape sequences (CSI, OSC) to get readable text for assertions.
 */
export function stripAnsi(text: string): string {
  return text
    .replace(/\x1b\[[?0-9;]*[a-zA-Z]/g, "")
    .replace(/\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)/g, "");
}

/**
 * Strip all whitespace characters from a string, collapsing it to a single
 * run of non-whitespace characters. Used for contiguous-echo assertions.
 */
export function compactNoWs(s: string): string {
  return Array.from(s)
    .filter((c) => !/\s/.test(c))
    .join("");
}

/**
 * Binary-search for the longest prefix of `expectedNoWs` that appears as a
 * contiguous substring inside `compact`. Returns the prefix length.
 */
export function longestContiguousPrefixLen(
  compact: string,
  expectedNoWs: string,
): number {
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

/**
 * Build a segmented echo payload of exactly `totalLen` characters, split into
 * `numSegments` chunks each prefixed with `#SEG-N:`. Returns `{ full, segments }`.
 */
export function buildLargeEchoSegmentedPayload(
  totalLen: number,
  numSegments: number,
): { full: string; segments: string[] } {
  const headers = Array.from({ length: numSegments }, (_, i) => `#SEG-${i}:`);
  const headerChars = headers.reduce((acc, h) => acc + h.length, 0);
  if (headerChars > totalLen) {
    throw new Error(
      `segment headers exceed totalLen=${totalLen} (headers use ${headerChars} chars, ${numSegments} segments)`,
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
