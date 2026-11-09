/**
 * Attachment-basename rules for the Start-session attach rows, checked before the form submits.
 *
 * An attachment is addressed by basename in a single flat level under `artifacts/attachments/`, so
 * the daemon refuses any name that is more than one path segment
 * (`session_attachments::validate_attachment_basename`, which reuses the uploads path's
 * `validate_segment` guard) and refuses a whole `StartSession` when two attachments collide on one
 * basename. Both refusals fail the entire session creation, naming neither the offending row nor
 * the other half of a collision — so the form applies the same rules itself and marks the row the
 * operator typed.
 *
 * This is a mirror of the host rule, never a replacement for it: every name accepted here is a name
 * the daemon accepts. Where the two differ this side is the stricter one — a whitespace-only name
 * would satisfy `validate_segment` but is refused here as empty, because it is not a name any
 * operator meant to type, and two names differing only in case are refused as a collision because
 * they collide on a case-insensitive host.
 *
 * Changeset: `2026-08-01-session-attach-ui`
 * PRD: docs/ft/coder/session-attachments.md
 */

/** Why a basename was refused, one code per rule the operator can be shown next to the row. */
export type AttachmentBasenameRejection =
  /** Nothing was typed, or nothing but whitespace. */
  | "empty"
  /** The name carries a path separator, so it is more than one segment. */
  | "separator"
  /** The name is `.` or `..`, which names a directory rather than a file. */
  | "dot-segment";

/** A basename is either usable as-is, or refused with the rule it broke. */
export type AttachmentBasenameValidation =
  | { ok: true }
  | { ok: false; reason: AttachmentBasenameRejection };

/** Separators the daemon's `validate_segment` rejects — both, on every platform. */
const PATH_SEPARATORS = ["/", "\\"];

/**
 * Validates one attachment basename as a single safe path segment.
 *
 * A separator is reported ahead of a dot segment, so `../escaped.md` reads as the path it is rather
 * than as its first component.
 */
export function validateAttachmentBasename(name: string): AttachmentBasenameValidation {
  if (name.trim() === "") {
    return { ok: false, reason: "empty" };
  }
  if (PATH_SEPARATORS.some((separator) => name.includes(separator))) {
    return { ok: false, reason: "separator" };
  }
  if (name === "." || name === "..") {
    return { ok: false, reason: "dot-segment" };
  }
  return { ok: true };
}

/**
 * The basenames shared by more than one row, each reported once and sorted so the refusal message is
 * stable across renders.
 *
 * Compared case-insensitively, because the collision is a filesystem collision rather than a string
 * one: the daemon writes each attachment with `create_new(true)`
 * (`session_attachments.rs`), so on a case-insensitive volume — the default on macOS — the second of
 * `Spec.md` and `spec.md` lands on the file the first one wrote and the whole `StartSession` fails
 * with *"an attachment with this name already exists"*. Folding case here is the stricter direction,
 * which is the only safe one: it refuses a pair the host would accept on Linux, rather than accepting
 * a pair that breaks creation on macOS.
 *
 * Each collision is reported under the casing of the first row that used it, so the message always
 * names a row the operator can see and edit.
 */
export function duplicateBasenames(names: string[]): string[] {
  /** folded name → the casing the first row carrying it was typed in. */
  const firstSeen = new Map<string, string>();
  const duplicates = new Map<string, string>();
  for (const name of names) {
    // `toLowerCase`, not `toLocaleLowerCase`: locale-independent folding, so the same two names
    // collide for every operator regardless of their browser locale.
    const folded = name.toLowerCase();
    const first = firstSeen.get(folded);
    if (first === undefined) {
      firstSeen.set(folded, name);
      continue;
    }
    duplicates.set(folded, first);
  }
  return [...duplicates.values()].sort();
}
