import type { CodeLocation } from "../../gen/code_navigation_pb";

/** A one-based line and one-based byte column in a worktree file — the coordinates the daemon speaks. */
export type CodePosition = { line: number; column: number };

/** A place a navigation answer points at, relative to the worktree root. */
export type CodeLocationRef = {
  relPath: string;
  start: CodePosition;
  end: CodePosition;
  /** Outside the worktree (a dependency, the standard library): the pane cannot open it. */
  outsideWorktree: boolean;
};

/** A `code_navigation.CodeLocation` in the pane's terms; a location without a range is malformed. */
export function toLocationRef(location: CodeLocation): CodeLocationRef {
  const { range } = location;
  if (!range?.start || !range.end) {
    throw new Error(`Navigation answer for ${location.relPath} carries no range`);
  }
  return {
    relPath: location.relPath,
    start: { line: range.start.line, column: range.start.column },
    end: { line: range.end.line, column: range.end.column },
    outsideWorktree: location.outsideWorktree,
  };
}
