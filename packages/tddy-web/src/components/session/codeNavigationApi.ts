import type { Client } from "@connectrpc/connect";
import type { CodeLocation, CodeNavigationService } from "../../gen/code_navigation_pb";
import type { WorktreeFilesApiConfig } from "./worktreeFilesApi";

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

/**
 * Thin data-access adapter over `code_navigation.CodeNavigationService`, in the shape of
 * `WorktreeFilesApi`: the session token, project id and worktree path are bound once, so the pane
 * speaks only worktree-relative paths and positions.
 */
export interface CodeNavigationApi {
  /** Where the symbol at `at` in `relPath` is defined. */
  definition(relPath: string, at: CodePosition): Promise<CodeLocationRef[]>;
  /** Every reference to the symbol at `at` in `relPath`, its declaration included. */
  references(relPath: string, at: CodePosition): Promise<CodeLocationRef[]>;
  /** The hover markdown of the symbol at `at` in `relPath`, or `null` when there is none. */
  hover(relPath: string, at: CodePosition): Promise<string | null>;
}

export type CodeNavigationApiConfig = WorktreeFilesApiConfig;

function toLocationRef(location: CodeLocation): CodeLocationRef {
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

export function createCodeNavigationApi(
  client: Client<typeof CodeNavigationService>,
  { sessionToken, projectId, worktreePath }: CodeNavigationApiConfig,
): CodeNavigationApi {
  const request = (relPath: string, position: CodePosition) => ({
    sessionToken,
    projectId,
    worktreePath,
    relPath,
    position,
  });
  return {
    async definition(relPath, at) {
      const res = await client.definition(request(relPath, at));
      return res.locations.map(toLocationRef);
    },
    async references(relPath, at) {
      const res = await client.references(request(relPath, at));
      return res.locations.map(toLocationRef);
    },
    async hover(relPath, at) {
      const res = await client.hover(request(relPath, at));
      return res.markdown ?? null;
    },
  };
}
