import { describe, it, expect } from "bun:test";
import { duplicateBasenames, validateAttachmentBasename } from "./attachmentBasenames";

/**
 * The form refuses a bad attachment name before submitting, so the operator sees the problem next to
 * the row they typed it in rather than as a failed session creation. These mirror the daemon's own
 * rules (`session_attachments::validate_attachment_basename`) — a name this accepts must be a name
 * the host accepts, or the check is worse than useless.
 */
describe("validateAttachmentBasename", () => {
  it("accepts an ordinary file name", () => {
    // Given / When
    const result = validateAttachmentBasename("requirements.pdf");

    // Then
    expect(result).toEqual({ ok: true });
  });

  it("accepts a name with spaces and unicode", () => {
    // Given / When
    const result = validateAttachmentBasename("design notes — v2.md");

    // Then
    expect(result).toEqual({ ok: true });
  });

  it("rejects an empty name as empty", () => {
    // Given / When
    const result = validateAttachmentBasename("");

    // Then
    expect(result).toEqual({ ok: false, reason: "empty" });
  });

  it("rejects a whitespace-only name as empty", () => {
    // Given / When
    const result = validateAttachmentBasename("   ");

    // Then
    expect(result).toEqual({ ok: false, reason: "empty" });
  });

  it("rejects a forward-slash path as a separator", () => {
    // Given / When
    const result = validateAttachmentBasename("docs/spec.md");

    // Then
    expect(result).toEqual({ ok: false, reason: "separator" });
  });

  it("rejects a backslash path as a separator", () => {
    // Given / When
    const result = validateAttachmentBasename("docs\\spec.md");

    // Then
    expect(result).toEqual({ ok: false, reason: "separator" });
  });

  it("rejects a leading-slash absolute path as a separator", () => {
    // Given / When
    const result = validateAttachmentBasename("/etc/passwd");

    // Then
    expect(result).toEqual({ ok: false, reason: "separator" });
  });

  it("rejects the current-directory segment as a dot segment", () => {
    // Given / When
    const result = validateAttachmentBasename(".");

    // Then
    expect(result).toEqual({ ok: false, reason: "dot-segment" });
  });

  it("rejects the parent-directory segment as a dot segment", () => {
    // Given / When
    const result = validateAttachmentBasename("..");

    // Then
    expect(result).toEqual({ ok: false, reason: "dot-segment" });
  });

  it("accepts a dotfile, which is a legitimate single segment", () => {
    // Given / When
    const result = validateAttachmentBasename(".env.example");

    // Then
    expect(result).toEqual({ ok: true });
  });
});

/**
 * The daemon rejects a whole `StartSession` when two attachments share a basename, so the form finds
 * the collision itself — otherwise a rename that happens to collide fails the entire creation with
 * no indication of which two rows caused it.
 */
describe("duplicateBasenames", () => {
  it("reports nothing when every name is distinct", () => {
    // Given / When
    const duplicates = duplicateBasenames(["a.md", "b.md", "c.md"]);

    // Then
    expect(duplicates).toEqual([]);
  });

  it("reports the one name shared by two rows", () => {
    // Given / When
    const duplicates = duplicateBasenames(["spec.md", "log.txt", "spec.md"]);

    // Then
    expect(duplicates).toEqual(["spec.md"]);
  });

  it("reports each duplicated name once regardless of how many rows share it", () => {
    // Given / When
    const duplicates = duplicateBasenames(["a.md", "a.md", "a.md"]);

    // Then
    expect(duplicates).toEqual(["a.md"]);
  });

  it("reports every duplicated name, sorted", () => {
    // Given / When
    const duplicates = duplicateBasenames(["z.md", "a.md", "z.md", "a.md", "m.md"]);

    // Then
    expect(duplicates).toEqual(["a.md", "z.md"]);
  });

  it("reports two names differing only by case as one collision", () => {
    // Given — macOS volumes are case-insensitive by default, and the daemon writes each attachment
    // with `create_new(true)`: the second of these would land on the file the first wrote and fail
    // the whole StartSession with "an attachment with this name already exists".
    // When
    const duplicates = duplicateBasenames(["Spec.md", "spec.md"]);

    // Then — named in the casing of the row that claimed it first, which the operator can see
    expect(duplicates).toEqual(["Spec.md"]);
  });

  it("reports one collision for three rows whose names differ only by case", () => {
    // Given / When
    const duplicates = duplicateBasenames(["Notes.md", "notes.md", "NOTES.MD"]);

    // Then
    expect(duplicates).toEqual(["Notes.md"]);
  });

  it("reports nothing for an empty list", () => {
    // Given / When
    const duplicates = duplicateBasenames([]);

    // Then
    expect(duplicates).toEqual([]);
  });
});
