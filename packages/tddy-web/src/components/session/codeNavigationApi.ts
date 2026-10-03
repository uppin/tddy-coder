import type { Client } from "@connectrpc/connect";
import type { CodeNavigationService } from "../../gen/code_navigation_pb";
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

export function createCodeNavigationApi(
  _client: Client<typeof CodeNavigationService>,
  _config: CodeNavigationApiConfig,
): CodeNavigationApi {
  // TODO(code-navigation): call `definition` / `references` / `hover` with the bound session
  // token, project id and worktree path, and map `CodeLocation` / `HoverResponse` into these shapes.
  const notWiredYet = (method: string) =>
    Promise.reject(new Error(`TODO(code-navigation): ${method} is not wired yet`));
  return {
    definition: () => notWiredYet("definition"),
    references: () => notWiredYet("references"),
    hover: () => notWiredYet("hover"),
  };
}
