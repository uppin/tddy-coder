import type { Client } from "@connectrpc/connect";
import type { ConnectionService } from "../../gen/connection_pb";

/** A single directory entry in the worktree tree. */
export type WorktreeEntry = { name: string; isDir: boolean };

/** The content of a single worktree file read on demand. */
export type WorktreeFileContent = {
  contentUtf8: string;
  truncated: boolean;
  byteSize: bigint;
};

/**
 * Thin data-access adapter over the `ConnectionService` worktree RPCs. Keeps all RPC wiring
 * (session token, project id, worktree path plumbing) out of the tree/preview components — they
 * only speak `listDir(relPath)` / `readFile(relPath)` in terms of worktree-relative paths.
 */
export interface WorktreeFilesApi {
  /** List one directory level, relative to the worktree root (empty string = root). */
  listDir(relPath: string): Promise<WorktreeEntry[]>;
  /** Read a single file, relative to the worktree root. */
  readFile(relPath: string): Promise<WorktreeFileContent>;
}

export type WorktreeFilesApiConfig = {
  sessionToken: string;
  projectId: string;
  worktreePath: string;
};

export function createWorktreeFilesApi(
  client: Client<typeof ConnectionService>,
  { sessionToken, projectId, worktreePath }: WorktreeFilesApiConfig,
): WorktreeFilesApi {
  return {
    async listDir(relPath) {
      const res = await client.listWorktreeDirectory({
        sessionToken,
        projectId,
        worktreePath,
        relPath,
      });
      return res.entries.map((e) => ({ name: e.name, isDir: e.isDir }));
    },
    async readFile(relPath) {
      const res = await client.readWorktreeFile({
        sessionToken,
        projectId,
        worktreePath,
        relPath,
      });
      return {
        contentUtf8: res.contentUtf8,
        truncated: res.truncated,
        byteSize: res.byteSize,
      };
    },
  };
}
