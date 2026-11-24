import React, { useState } from "react";
import {
  CloseSessionWorktreeOwner,
  type CloseSessionPreview,
  type CloseSessionWorktree,
  type SessionEntry,
} from "tddy-client-web/gen/session_pb";
import { rpcFailureText } from "tddy-client-web";
import { Button, InlineError } from "tddy-ui-web";
import { sessionDrawerLabel } from "./utils/sessionDrawerLabel";
import type { CloseConfirmation, CloseDialogRequest } from "./useSessionCloses";

export interface CloseSessionDialogProps {
  /** The close waiting on the operator, or `null` when none is. */
  request: CloseDialogRequest | null;
  /** Ask a session's owning host what closing it would lose. Rejects with the RPC failure. */
  onPreview: (session: SessionEntry) => Promise<CloseSessionPreview>;
  onCancel: (request: CloseDialogRequest) => void;
  onConfirm: (request: CloseDialogRequest, confirmation: CloseConfirmation) => void;
}

/** A child's preview, as far as it has got. */
type ChildPreview =
  | { readonly state: "checking" }
  | { readonly state: "ready"; readonly preview: CloseSessionPreview }
  | { readonly state: "failed"; readonly reason: string };

const readable = (worktree: CloseSessionWorktree) => worktree.unavailableReason === "";

/**
 * What closing a session would lose, shown before anything is terminated or removed
 * (docs/ft/web/session-drawer.md § Closing a session): one section per worktree, the children
 * choice when the session started others, and Confirm — disabled while any worktree could not be
 * read, so a close never proceeds blind.
 *
 * Follows the hand-rolled overlay of `DirtyWorktreeDialog` (there is no shared dialog component).
 * Keyed by the request, so a dialog re-opened by the daemon starts from its own preview.
 */
export function CloseSessionDialog({ request, onPreview, onCancel, onConfirm }: CloseSessionDialogProps) {
  if (!request) return null;
  return (
    <CloseSessionDialogBody
      key={request.key}
      request={request}
      onPreview={onPreview}
      onCancel={onCancel}
      onConfirm={onConfirm}
    />
  );
}

function CloseSessionDialogBody({
  request,
  onPreview,
  onCancel,
  onConfirm,
}: CloseSessionDialogProps & { request: CloseDialogRequest }) {
  const { session, children } = request;
  const label = sessionDrawerLabel(session);
  const [preview, setPreview] = useState(request.preview);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const [closeChildren, setCloseChildren] = useState(false);
  const [childPreviews, setChildPreviews] = useState<ReadonlyMap<string, ChildPreview>>(() => new Map());

  const setChildPreview = (sessionId: string, childPreview: ChildPreview) =>
    setChildPreviews((prev) => new Map(prev).set(sessionId, childPreview));

  const previewChildren = () => {
    for (const child of children) {
      setChildPreview(child.sessionId, { state: "checking" });
      onPreview(child).then(
        (childPreview) => setChildPreview(child.sessionId, { state: "ready", preview: childPreview }),
        (err: unknown) => setChildPreview(child.sessionId, { state: "failed", reason: rpcFailureText(err) }),
      );
    }
  };

  const chooseCloseChildren = (value: boolean) => {
    setCloseChildren(value);
    if (value) previewChildren();
  };

  const retry = async () => {
    if (closeChildren) previewChildren();
    setChecking(true);
    setPreviewError(null);
    try {
      setPreview(await onPreview(session));
    } catch (err) {
      setPreviewError(rpcFailureText(err));
    } finally {
      setChecking(false);
    }
  };

  const chosenChildren = closeChildren ? children : [];
  const childReady = (child: SessionEntry) => {
    const childPreview = childPreviews.get(child.sessionId);
    return childPreview?.state === "ready" && childPreview.preview.worktrees.every(readable);
  };
  const somethingUnreadable =
    !preview.worktrees.every(readable) ||
    previewError !== null ||
    chosenChildren.some((child) => {
      const childPreview = childPreviews.get(child.sessionId);
      return childPreview?.state === "failed" || (childPreview?.state === "ready" && !childReady(child));
    });
  const canConfirm = !checking && preview.worktrees.every(readable) && chosenChildren.every(childReady);

  const confirm = () => {
    const confirmation: CloseConfirmation = {
      fingerprint: preview.fingerprint,
      children: chosenChildren.map((child) => {
        const childPreview = childPreviews.get(child.sessionId);
        return { session: child, fingerprint: childPreview?.state === "ready" ? childPreview.preview.fingerprint : "" };
      }),
    };
    onConfirm(request, confirmation);
  };

  // Sections are numbered across the whole dialog: the session's own worktrees, then each chosen
  // child's, in order.
  const childFirstSection = new Map<string, number>();
  let sectionCount = preview.worktrees.length;
  for (const child of chosenChildren) {
    childFirstSection.set(child.sessionId, sectionCount);
    const childPreview = childPreviews.get(child.sessionId);
    if (childPreview?.state === "ready") sectionCount += childPreview.preview.worktrees.length;
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
      data-testid="close-session-dialog"
      role="dialog"
      aria-modal="true"
      aria-label={`Close session ${label}`}
    >
      <div className="bg-card text-card-foreground border-border flex max-h-[90vh] w-full max-w-lg flex-col gap-3 overflow-hidden rounded-xl border p-4 shadow-lg">
        <h2 className="text-sm font-semibold">Close {label}?</h2>
        <p className="text-xs text-muted-foreground">
          Closing terminates every agent tab of this session and removes every worktree it owns. The
          session's record and branch are kept.
        </p>
        <div className="flex min-h-0 flex-col gap-2 overflow-y-auto">
          <CloseSessionTabs preview={preview} />
          <CloseSessionWorktreeSections worktrees={preview.worktrees} firstIndex={0} />
          <InlineError testId="close-session-dialog-preview-error" message={previewError} className="text-xs" />
          {children.length > 0 && (
            <CloseSessionChildrenChoice
              dialogKey={request.key}
              startedSessions={children}
              closeChildren={closeChildren}
              onChoose={chooseCloseChildren}
            />
          )}
          {chosenChildren.map((child) => (
            <CloseSessionChildLosses
              key={child.sessionId}
              child={child}
              childPreview={childPreviews.get(child.sessionId)}
              firstSection={childFirstSection.get(child.sessionId) ?? 0}
            />
          ))}
        </div>
        <div className="flex justify-end gap-2">
          {somethingUnreadable && (
            <Button
              data-testid="close-session-dialog-retry"
              size="sm"
              variant="outline"
              disabled={checking}
              onClick={() => void retry()}
            >
              Retry
            </Button>
          )}
          <Button data-testid="close-session-dialog-cancel" size="sm" variant="outline" onClick={() => onCancel(request)}>
            Cancel
          </Button>
          <Button
            data-testid="close-session-dialog-confirm"
            size="sm"
            variant="destructive"
            disabled={!canConfirm}
            onClick={confirm}
          >
            Terminate and remove
          </Button>
        </div>
      </div>
    </div>
  );
}

/** The agent tabs the close would terminate. */
function CloseSessionTabs({ preview }: { preview: CloseSessionPreview }) {
  if (preview.tabs.length === 0) return null;
  return (
    <p className="text-xs text-muted-foreground">
      Terminates {preview.tabs.map((tab) => tab.label || tab.tabId).join(", ")}.
    </p>
  );
}

/** Whose a worktree is, as the operator knows it. */
function worktreeOwnerText(worktree: CloseSessionWorktree): string {
  switch (worktree.owner) {
    case CloseSessionWorktreeOwner.CONVERSATION:
      return `conversation ${worktree.ownerId}`;
    case CloseSessionWorktreeOwner.CODEBASE:
      return worktree.ownerId ? `codebase half ${worktree.ownerId}` : "codebase half";
    default:
      return "the session";
  }
}

/** One section per worktree, numbered from `firstIndex`. */
function CloseSessionWorktreeSections({
  worktrees,
  firstIndex,
}: {
  worktrees: readonly CloseSessionWorktree[];
  firstIndex: number;
}) {
  return (
    <>
      {worktrees.map((worktree, offset) => (
        <CloseSessionWorktreeSection key={worktree.path} index={firstIndex + offset} worktree={worktree} />
      ))}
    </>
  );
}

/** One worktree the close would remove: where it is, whose, its branch, and what removing it loses. */
function CloseSessionWorktreeSection({ index, worktree }: { index: number; worktree: CloseSessionWorktree }) {
  const unlistedCommits = worktree.unpushedCommitCount - worktree.unpushedCommits.length;
  const loses = worktree.dirtyFiles.length > 0 || worktree.unpushedCommitCount > 0;

  return (
    <section
      data-testid={`close-session-dialog-worktree-${index}`}
      className="space-y-1 rounded-md border border-border p-2 text-xs"
    >
      <div className="flex flex-wrap items-baseline gap-x-2">
        <span className="font-mono font-medium">{worktree.branch || "(no branch)"}</span>
        <span className="text-muted-foreground">{worktreeOwnerText(worktree)}</span>
        {worktree.host && <span className="text-muted-foreground">on {worktree.host}</span>}
      </div>
      <div className="truncate font-mono text-muted-foreground" title={worktree.path}>
        {worktree.path}
      </div>
      {worktree.unavailableReason ? (
        <p role="alert" className="text-destructive">
          Could not be read: {worktree.unavailableReason}
        </p>
      ) : (
        <>
          {worktree.dirtyFiles.length > 0 && (
            <div>
              <p className="font-medium">Uncommitted files</p>
              <ul className="font-mono">
                {worktree.dirtyFiles.map((file) => (
                  <li key={file.path} className="flex gap-2">
                    <span className="min-w-0 truncate">{file.path}</span>
                    <span className="flex-shrink-0 text-muted-foreground">{file.change}</span>
                  </li>
                ))}
              </ul>
            </div>
          )}
          {worktree.unpushedCommitCount > 0 && (
            <div>
              <p className="font-medium">Commits on no remote</p>
              <ul>
                {worktree.unpushedCommits.map((commit) => (
                  <li key={commit.sha} className="flex gap-2">
                    <span className="flex-shrink-0 font-mono text-muted-foreground">{commit.sha.slice(0, 7)}</span>
                    <span className="min-w-0 truncate">{commit.subject}</span>
                  </li>
                ))}
              </ul>
              {unlistedCommits > 0 && (
                <p className="text-muted-foreground">
                  and {unlistedCommits} more {unlistedCommits === 1 ? "commit" : "commits"} on no remote
                </p>
              )}
            </div>
          )}
          {!loses && <p className="text-muted-foreground">Nothing uncommitted or unpushed.</p>}
        </>
      )}
    </section>
  );
}

/** What to do with the sessions this one started: leave them running (default) or close them too. */
function CloseSessionChildrenChoice({
  dialogKey,
  startedSessions,
  closeChildren,
  onChoose,
}: {
  dialogKey: number;
  startedSessions: readonly SessionEntry[];
  closeChildren: boolean;
  onChoose: (closeChildren: boolean) => void;
}) {
  const name = `close-session-children-${dialogKey}`;
  return (
    <fieldset className="space-y-1 rounded-md border border-border p-2 text-xs">
      <legend className="px-1 font-medium">
        This session started {startedSessions.length} other{" "}
        {startedSessions.length === 1 ? "session" : "sessions"}
      </legend>
      <label className="flex items-center gap-2">
        <input
          type="radio"
          name={name}
          data-testid="close-session-dialog-children-leave"
          checked={!closeChildren}
          onChange={() => onChoose(false)}
        />
        Leave them running
      </label>
      <label className="flex items-center gap-2">
        <input
          type="radio"
          name={name}
          data-testid="close-session-dialog-children-close"
          checked={closeChildren}
          onChange={() => onChoose(true)}
        />
        Close them too
      </label>
    </fieldset>
  );
}

/** A child chosen to close too: its own losses, once its host has said what they are. */
function CloseSessionChildLosses({
  child,
  childPreview,
  firstSection,
}: {
  child: SessionEntry;
  childPreview: ChildPreview | undefined;
  /** The dialog-wide number of this child's first worktree section. */
  firstSection: number;
}) {
  return (
    <div className="space-y-1">
      <h3 className="text-xs font-semibold">{sessionDrawerLabel(child)}</h3>
      {(childPreview === undefined || childPreview.state === "checking") && (
        <p className="text-xs text-muted-foreground">Checking what closing it would lose…</p>
      )}
      {childPreview?.state === "failed" && (
        <InlineError
          testId={`close-session-dialog-child-error-${child.sessionId}`}
          message={childPreview.reason}
          className="text-xs"
        />
      )}
      {childPreview?.state === "ready" &&
        (childPreview.preview.worktrees.length > 0 ? (
          <CloseSessionWorktreeSections worktrees={childPreview.preview.worktrees} firstIndex={firstSection} />
        ) : (
          <p className="text-xs text-muted-foreground">Nothing to lose.</p>
        ))}
    </div>
  );
}
