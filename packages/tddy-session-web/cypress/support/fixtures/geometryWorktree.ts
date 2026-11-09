/**
 * The two-file Rust worktree the Code pane specs navigate: `src/main.rs` calls `geometry::area`,
 * which `src/geometry.rs` defines far down the file — plus the index phases those specs report.
 *
 * Positions are one-based lines and one-based byte columns, the daemon's coordinates.
 */

import {
  aCodeIndexHost,
  aCodeLocation,
  type CodeIndexHost,
  type IndexPhase,
  type SourcePoint,
} from "../rpc/codeSessionBackend";

// `src/main.rs` calls `geometry::area` on line 4; the call's `area` starts at byte column 26, and
// `main` on line 3 starts at byte column 4.
export const MAIN_RS = [
  "mod geometry;",
  "",
  "fn main() {",
  "    let area = geometry::area(3, 4);",
  '    println!("{area}");',
  "}",
  "",
].join("\n");

/** The call to `area` in `src/main.rs` — the index knows its type, definition and references. */
export const THE_CALL_TO_AREA = { line: 4, column: 26 };

/** `main` in `src/main.rs` — the index knows nothing about it: no hover, no definition. */
export const MAIN = { line: 3, column: 4 };

// `src/geometry.rs` defines `area` on line 120 — far enough down that opening the file at its top
// would leave the definition out of view, so "scrolled to its line" is observable.
export const DEFINITION_LINE = 120;
export const GEOMETRY_RS = [
  ...Array.from({ length: DEFINITION_LINE - 1 }, (_, i) => `// geometry note ${i + 1}`),
  "pub fn area(width: u32, height: u32) -> u32 {",
  "    width * height",
  "}",
  "",
].join("\n");

/** The text of the line defining `area`. */
export const THE_LINE_DEFINING_AREA = "pub fn area(width: u32, height: u32) -> u32 {";

/** The type the index answers a hover on `area` with. */
export const AREA_TYPE = "fn area(width: u32, height: u32) -> u32";

export const THE_DEFINITION_OF_AREA = aCodeLocation("src/geometry.rs", DEFINITION_LINE, 8, 12);
export const THE_CALL_IN_MAIN = aCodeLocation("src/main.rs", 4, 26, 30);

export const FETCHING_20: IndexPhase = { title: "Fetching", percentage: 20, message: "crates.io index" };
export const INDEXING_38: IndexPhase = {
  title: "Indexing",
  percentage: 38,
  message: "120/310 (tddy_core)",
};
export const INDEXING_61: IndexPhase = {
  title: "Indexing",
  percentage: 61,
  message: "190/310 (tddy_web)",
};

/** `at` in `src/main.rs`. */
export const inMainRs = (at: { line: number; column: number }): SourcePoint => ({
  relPath: "src/main.rs",
  ...at,
});

/**
 * A host for the two-file worktree whose index knows `area` — its type `hover` (default
 * `AREA_TYPE`), its definition and its references — and nothing about `main`.
 */
export function aGeometryWorktreeHost(hover: string = AREA_TYPE): CodeIndexHost {
  return aCodeIndexHost()
    .withFile("src/main.rs", MAIN_RS)
    .withFile("src/geometry.rs", GEOMETRY_RS)
    .knowing({
      at: inMainRs(THE_CALL_TO_AREA),
      hover,
      definition: [THE_DEFINITION_OF_AREA],
      references: [THE_DEFINITION_OF_AREA, THE_CALL_IN_MAIN],
    });
}
