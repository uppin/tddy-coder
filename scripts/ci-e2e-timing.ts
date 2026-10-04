/**
 * Where the e2e leg's time goes.
 *
 * Two readings, both from files CI already produces:
 *
 * - `junit <junit.xml>` — per-binary run time of the e2e leg, for the check's step summary.
 * - `compile-share <cargo-timing.html> <rust-e2e.filterset>` — how much of a `cargo test --no-run
 *   --timings` build went to e2e test targets, to the other test targets, and to everything else.
 *   It decides whether splitting the compile by test target is worth the manifest churn.
 *
 * Parsers take file *contents*, so they need no cargo and no repository.
 */

/** Share of a leg's compile that the leg would skip, at or above which the split is worth doing. */
export const SPLIT_WORTH_IT_PERCENT = 25;

export interface BinaryTiming {
  binary: string;
  seconds: number;
  tests: number;
}

export interface CompileShare {
  e2eTestSeconds: number;
  otherTestSeconds: number;
  otherSeconds: number;
  totalSeconds: number;
  /** What the e2e leg would stop compiling: every test target that is not e2e, as % of total. */
  e2eLegSkippablePercent: number;
  /** What the unit leg would stop compiling: every e2e test target, as % of total. */
  unitLegSkippablePercent: number;
  /** Filterset binaries that never appear as a test target in the report. */
  missingFromReport: string[];
}

export type Verdict = "proceed" | "stop";

/** Per-binary run time, slowest first. */
export function perBinaryTimings(junitXml: string): BinaryTiming[] {
  // TODO(compile-timings): implement
  void junitXml;
  throw new Error("perBinaryTimings is not implemented");
}

/** Compile time split into e2e test targets, other test targets and the rest. */
export function compileShare(timingsHtml: string, filterset: string): CompileShare {
  // TODO(compile-timings): implement
  void timingsHtml;
  void filterset;
  throw new Error("compileShare is not implemented");
}

/** `proceed` when the leg would skip at least {@link SPLIT_WORTH_IT_PERCENT}% of its compile. */
export function verdict(skippablePercent: number): Verdict {
  // TODO(compile-timings): implement
  void skippablePercent;
  throw new Error("verdict is not implemented");
}
