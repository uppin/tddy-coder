/**
 * Browses documents that already exist on a host and yields one as a `HostDocumentRef` — scope plus a
 * relative path, never an absolute host path, so a client cannot name an arbitrary file the daemon's
 * user happens to be able to read. Nothing is uploaded: the session host reads the bytes itself.
 *
 * Three traps this encodes:
 * - a `SessionContextDoc` of kind `ATTACHMENT` lives at `attachments/<basename>` under `artifacts/`,
 *   while a `MANIFEST` doc is the bare basename. Getting it wrong is silent — the daemon just reports
 *   the document as missing.
 * - a doc the recipe declared but never wrote (`exists === false`) is not offered at all; picking it
 *   could only earn a `NOT_FOUND` at materialization time.
 * - each scope has its own root, and a ref carries only the id that root is resolved from: a session
 *   for the `SESSION_*` scopes, a project for `PROJECT_REPO`. `relative_path` is relative to that root
 *   — including any directories above the file, which the tree scopes make possible.
 *
 * The host browsed is the one the form's client is connected to: `ListSessions`,
 * `ListSessionUploads` and `ListWorktreeDirectory` carry no `daemon_instance_id`, so a peer's
 * documents are not enumerable over this client (tracked in the changeset).
 *
 * Changeset: `2026-08-01-session-attach-ui`
 * Feature: docs/ft/coder/session-attachments.md § ReadHostDocument
 */

import React, { useEffect, useMemo, useState } from "react";
import type { Client } from "@connectrpc/connect";
import {
  HostDocumentScope,
  SessionContextDocKind,
  type ConnectionService,
  type SessionEntry,
  type SessionUploadEntry,
} from "../../../gen/connection_pb";
import { formatAttachmentBytes } from "../../../lib/attachmentBytes";
import { WorktreeFileTree } from "../../session/WorktreeFileTree";
import { createWorktreeFilesApi, type WorktreeFilesApi } from "../../session/worktreeFilesApi";
import type { HostDocumentSelection } from "./pendingAttachment";

/** The document the operator chose, ready to become a `SessionAttachment`. */
export interface HostDocumentPick {
  /** Name the attachment will be stored under — the document's own file name. */
  basename: string;
  sizeBytes: number;
  document: HostDocumentSelection;
}

export interface HostDocumentPickerProps {
  client: Client<typeof ConnectionService>;
  sessionToken: string;
  /**
   * The host being browsed — stamped on every ref this picker yields.
   *
   * **Invariant**: it must name the host `client` enumerates from. The listing RPCs
   * (`ListSessions`, `ListSessionUploads`) carry no `daemon_instance_id`, so they always answer for
   * whichever host the client is connected to; passing a different id here would list one host's
   * documents and stamp another's, producing refs to files that host does not have. The daemon then
   * reports the document as missing — a silent, late failure. Named for what it is rather than
   * `daemonInstanceId` so a future caller cannot read it as "any host".
   */
  browsedDaemonInstanceId: string;
  /**
   * The project whose repository the `PROJECT_REPO` scope browses — the form's selected project. Its
   * `mainRepoPath` is that scope's root, and the id is what a `PROJECT_REPO` ref resolves against on
   * the host. `undefined` while no project is selected, which leaves that scope with nothing to list.
   */
  project: { projectId: string; mainRepoPath: string } | undefined;
  onPick: (pick: HostDocumentPick) => void;
  onClose: () => void;
}

/** One offered document: what it would be referenced by, and what it would be stored as. */
interface HostDocumentRow {
  relativePath: string;
  basename: string;
  sizeBytes: number;
  description: string;
}

/**
 * The scopes this picker enumerates. The first two are flat lists that one RPC returns whole; the last
 * two are directory trees browsed one level at a time through `ListWorktreeDirectory`.
 */
const OFFERED_SCOPES: readonly { scope: HostDocumentScope; label: string }[] = [
  { scope: HostDocumentScope.SESSION_ARTIFACT, label: "Session artifacts" },
  { scope: HostDocumentScope.SESSION_UPLOAD, label: "Session uploads" },
  { scope: HostDocumentScope.SESSION_WORKTREE, label: "Session worktree" },
  { scope: HostDocumentScope.PROJECT_REPO, label: "Project repository" },
];

/** Whether a scope is rooted at a session, and so needs one selected before it can be listed. */
function isSessionScoped(scope: HostDocumentScope): boolean {
  return scope !== HostDocumentScope.PROJECT_REPO;
}

/** Whether a scope is browsed as a directory tree rather than listed flat by a single RPC. */
function isTreeScope(scope: HostDocumentScope): boolean {
  return scope === HostDocumentScope.SESSION_WORKTREE || scope === HostDocumentScope.PROJECT_REPO;
}

/**
 * The artifact-scope rows of one session: every context doc that is actually on disk, addressed
 * relative to `artifacts/` — which puts a user-attached doc under `attachments/`.
 */
function artifactRows(session: SessionEntry | undefined): HostDocumentRow[] {
  if (session === undefined) return [];
  return session.contextDocs
    .filter((doc) => doc.exists)
    .map((doc) => ({
      relativePath:
        doc.kind === SessionContextDocKind.ATTACHMENT
          ? `attachments/${doc.basename}`
          : doc.basename,
      basename: doc.basename,
      sizeBytes: Number(doc.sizeBytes),
      description: doc.description,
    }));
}

/** The upload-scope rows of one session: the `<upload_id>/<file_name>` pairs the daemon requires. */
function uploadRows(uploads: SessionUploadEntry[]): HostDocumentRow[] {
  return uploads.map((upload) => ({
    relativePath: `${upload.uploadId}/${upload.fileName}`,
    basename: upload.fileName,
    sizeBytes: Number(upload.sizeBytes),
    description: "Uploaded file",
  }));
}

/** The last segment of a scope-relative path — the name the attachment will be stored under. */
function basenameOf(relativePath: string): string {
  const segments = relativePath.split("/");
  return segments[segments.length - 1]!;
}

/** A directory tree to browse, plus the listed size of every file in it. */
interface BrowsableTree {
  api: WorktreeFilesApi;
  /** The size of a listed file, or `undefined` for a path no listing has offered. */
  sizeOf: (relativePath: string) => number | undefined;
}

/**
 * Wraps a worktree files API so every listed file's size is remembered, keyed by its path relative to
 * the tree root.
 *
 * `WorktreeFileTree` reports only the *path* of the file the operator picked, so the size the cap
 * check needs is captured from the very listing that produced the node. That beats re-reading it at
 * pick time: it is already on the wire (`WorktreeDirEntry.size_bytes`), and a second call could
 * answer differently from what was offered.
 */
function browsableTree(base: WorktreeFilesApi): BrowsableTree {
  const sizes = new Map<string, number>();
  return {
    api: {
      readFile: (relPath) => base.readFile(relPath),
      listDir: async (relPath) => {
        const entries = await base.listDir(relPath);
        for (const entry of entries) {
          if (entry.isDir) continue;
          sizes.set(relPath === "" ? entry.name : `${relPath}/${entry.name}`, entry.sizeBytes);
        }
        return entries;
      },
    },
    sizeOf: (relativePath) => sizes.get(relativePath),
  };
}

export function HostDocumentPicker({
  client,
  sessionToken,
  browsedDaemonInstanceId,
  project,
  onPick,
  onClose,
}: HostDocumentPickerProps) {
  const [scope, setScope] = useState<HostDocumentScope>(HostDocumentScope.SESSION_ARTIFACT);
  const [sessions, setSessions] = useState<SessionEntry[]>([]);
  const [sessionId, setSessionId] = useState("");
  const [uploads, setUploads] = useState<SessionUploadEntry[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    client
      .listSessions({ sessionToken })
      .then((resp) => {
        if (cancelled) return;
        const loaded = resp.sessions as SessionEntry[];
        setSessions(loaded);
        // Auto-select when there is exactly one session — no decision to make.
        if (loaded.length === 1) setSessionId(loaded[0]!.sessionId);
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(err instanceof Error ? err.message : String(err));
      });
    return () => {
      cancelled = true;
    };
  }, [client, sessionToken]);

  useEffect(() => {
    if (scope !== HostDocumentScope.SESSION_UPLOAD || sessionId === "") {
      setUploads([]);
      return;
    }
    let cancelled = false;
    client
      .listSessionUploads({ sessionToken, sessionId })
      .then((resp) => {
        if (!cancelled) setUploads(resp.uploads as SessionUploadEntry[]);
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(err instanceof Error ? err.message : String(err));
      });
    return () => {
      cancelled = true;
    };
  }, [client, sessionToken, scope, sessionId]);

  const selectedSession = sessions.find((s) => s.sessionId === sessionId);

  const rows = useMemo(() => {
    if (scope === HostDocumentScope.SESSION_UPLOAD) return uploadRows(uploads);
    if (scope === HostDocumentScope.SESSION_ARTIFACT) return artifactRows(selectedSession);
    // The tree scopes list themselves, one directory at a time.
    return [];
  }, [scope, uploads, selectedSession]);

  // Which worktree the tree scopes browse, and which project the listing RPC resolves it against —
  // `ListWorktreeDirectory` only accepts a path git lists as a worktree of that project's main repo.
  // A session's worktree therefore belongs to the *session's* project, which is not necessarily the
  // one the form is creating in; the project-repo scope lists the project's primary worktree.
  const treeSource = useMemo(() => {
    if (scope === HostDocumentScope.SESSION_WORKTREE && selectedSession !== undefined) {
      return { projectId: selectedSession.projectId, worktreePath: selectedSession.repoPath };
    }
    if (scope === HostDocumentScope.PROJECT_REPO && project !== undefined) {
      return { projectId: project.projectId, worktreePath: project.mainRepoPath };
    }
    return undefined;
  }, [scope, selectedSession, project]);

  const tree = useMemo(
    () =>
      treeSource === undefined || treeSource.worktreePath === ""
        ? undefined
        : browsableTree(createWorktreeFilesApi(client, { sessionToken, ...treeSource })),
    [client, sessionToken, treeSource],
  );

  /**
   * How the picked path is addressed on the host. Only one of the two ids is carried, because only one
   * of them is the scope's root: naming both would be a claim the host cannot check, and naming the
   * wrong one resolves to a root the file is not under — a silent `NOT_FOUND` at materialization time.
   */
  const documentRef = (relativePath: string): HostDocumentSelection => ({
    daemonInstanceId: browsedDaemonInstanceId,
    scope,
    sessionId: isSessionScoped(scope) ? sessionId : "",
    projectId: scope === HostDocumentScope.PROJECT_REPO ? (project?.projectId ?? "") : "",
    relativePath,
  });

  const handlePick = (row: HostDocumentRow) => {
    onPick({
      basename: row.basename,
      sizeBytes: row.sizeBytes,
      document: documentRef(row.relativePath),
    });
  };

  const handlePickFromTree = (relPath: string) => {
    const sizeBytes = tree?.sizeOf(relPath);
    if (sizeBytes === undefined) {
      // Unreachable: a tree node exists only because `listDir` returned its entry, and every listed
      // file's size is recorded. Raised rather than picked with an assumed size, which would slip an
      // unmeasured document past the attachment cap.
      throw new Error(`“${relPath}” was listed without a size, so it cannot be attached`);
    }
    onPick({ basename: basenameOf(relPath), sizeBytes, document: documentRef(relPath) });
  };

  return (
    <div
      data-testid="create-session-host-doc-picker"
      className="rounded-md border border-input bg-background p-2 space-y-2"
    >
      <div className="flex items-center gap-2">
        <select
          aria-label="Which documents to browse"
          data-testid="create-session-host-doc-picker-scope-select"
          className="rounded-md border border-input bg-background px-2 py-1 text-xs"
          value={String(scope)}
          onChange={(e) => setScope(Number(e.target.value) as HostDocumentScope)}
        >
          {OFFERED_SCOPES.map((offered) => (
            <option key={offered.scope} value={String(offered.scope)}>
              {offered.label}
            </option>
          ))}
        </select>
        {isSessionScoped(scope) ? (
          <select
            aria-label="Session holding the documents"
            className="min-w-0 flex-1 rounded-md border border-input bg-background px-2 py-1 text-xs"
            value={sessionId}
            onChange={(e) => setSessionId(e.target.value)}
          >
            <option value="" disabled>
              {sessions.length === 0 ? "No sessions on this host" : "Select a session…"}
            </option>
            {sessions.map((session) => (
              <option key={session.sessionId} value={session.sessionId}>
                {session.sessionId}
              </option>
            ))}
          </select>
        ) : (
          // The project-repo scope has no session to choose; the spacer keeps Close on the right.
          <span className="min-w-0 flex-1" />
        )}
        <button
          type="button"
          className="rounded-md border border-input px-2 py-1 text-xs hover:bg-muted"
          onClick={onClose}
        >
          Close
        </button>
      </div>

      {error !== null && <p className="text-xs text-destructive">{error}</p>}

      {isTreeScope(scope) ? (
        tree === undefined ? (
          <p className="text-xs text-muted-foreground">
            {scope === HostDocumentScope.PROJECT_REPO
              ? "Select a project to browse its repository."
              : "Select a session with a worktree to browse it."}
          </p>
        ) : (
          <div className="max-h-64 overflow-auto">
            <WorktreeFileTree
              api={tree.api}
              selectedRelPath={null}
              onSelectFile={handlePickFromTree}
            />
          </div>
        )
      ) : rows.length === 0 ? (
        <p className="text-xs text-muted-foreground">No documents to attach in this scope.</p>
      ) : (
        <ul className="space-y-1">
          {rows.map((row) => (
            <li key={row.relativePath}>
              <button
                type="button"
                data-testid={`create-session-host-doc-row-${row.relativePath}`}
                className="flex w-full items-center gap-2 rounded border border-input px-2 py-1 text-left text-xs hover:bg-muted"
                onClick={() => handlePick(row)}
              >
                <span className="min-w-0 flex-1 truncate">{row.relativePath}</span>
                <span className="shrink-0 text-muted-foreground">{row.description}</span>
                <span className="shrink-0 text-muted-foreground">
                  {formatAttachmentBytes(row.sizeBytes)}
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
