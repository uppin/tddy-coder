/**
 * The attach-row model of the new-session form: a document the operator has attached but that has
 * not been sent anywhere yet. Nothing is uploaded until the form is submitted, so a row holds either
 * the local `File` itself or a reference to a document that already lives on a host.
 *
 * Changeset: `2026-08-01-session-attach-ui`
 * Feature: docs/ft/coder/session-attachments.md
 */

import { HostDocumentScope } from "../../../gen/connection_pb";

/** A document that already exists on a host, addressed the way `HostDocumentRef` addresses it. */
export interface HostDocumentSelection {
  /** Host that owns the bytes; it performs the read under its own os_user mapping. */
  daemonInstanceId: string;
  scope: HostDocumentScope;
  /** Owning session for the `SESSION_*` scopes. */
  sessionId: string;
  /** Owning project for `PROJECT_REPO`. */
  projectId: string;
  /** Path relative to the scope's root — never an absolute host path. */
  relativePath: string;
}

/** Where an attached document's bytes come from. Mirrors `SessionAttachment.source`. */
export type PendingAttachmentSource =
  | { case: "file"; file: File }
  | { case: "hostDocument"; document: HostDocumentSelection };

/** One row of the form's attachments section. */
export interface PendingAttachment {
  /**
   * Row identity, stable across a rename — the basename is what the operator edits, so it cannot
   * also be the React key without remounting (and so losing focus in) the input being typed into.
   */
  id: string;
  /** Name the host will materialize the document under (`SessionAttachment.basename`). */
  basename: string;
  /** Size the host will store. Known before submit for both sources. */
  sizeBytes: number;
  source: PendingAttachmentSource;
}

/**
 * An attach row a caller opens the form with — a {@link PendingAttachment} without the row identity,
 * which the form assigns. Pre-populated rows are a *default*, not an invariant: they render, rename
 * and remove like any other row.
 */
export type InitialAttachment = Omit<PendingAttachment, "id">;

/** How far one attachment has got, and which half of the flow it is in. */
export interface AttachmentProgress {
  /** `0`–`100`, rounded. Zero while a byte count is not yet known. */
  percent: number;
  phase: "staging" | "materializing";
}

/** Per-attachment progress, keyed by basename — the only identity the host's events carry. */
export type AttachmentProgressByBasename = Readonly<Record<string, AttachmentProgress>>;

const SCOPE_LABELS: Readonly<Record<HostDocumentScope, string>> = {
  [HostDocumentScope.UNSPECIFIED]: "Host document",
  [HostDocumentScope.SESSION_ARTIFACT]: "Session artifact",
  [HostDocumentScope.SESSION_UPLOAD]: "Session upload",
  [HostDocumentScope.SESSION_WORKTREE]: "Worktree file",
  [HostDocumentScope.PROJECT_REPO]: "Project file",
  [HostDocumentScope.STAGED_ATTACHMENT]: "Staged file",
};

/** Short human label for a row's source, so an operator can tell an upload from a reference. */
export function attachmentSourceLabel(source: PendingAttachmentSource): string {
  if (source.case === "file") return "Local file";
  return SCOPE_LABELS[source.document.scope];
}
