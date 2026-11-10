/**
 * Unit tests for POSIX shell quoting of uploaded file paths, used when a dropped
 * file's host path is typed into the terminal.
 *
 * Changeset: `terminal-file-drop-upload`
 * PRD: docs/ft/web/web-terminal.md § File drop upload
 */

import { describe, it, expect } from "bun:test";
import { shellQuotePath, joinQuotedPaths } from "./shellQuote";

describe("shellQuotePath", () => {
  it("wraps a plain path in single quotes", () => {
    expect(shellQuotePath("/home/tddy/report.pdf")).toBe("'/home/tddy/report.pdf'");
  });

  it("keeps a path with spaces as a single shell token", () => {
    expect(shellQuotePath("/home/tddy/my report.pdf")).toBe("'/home/tddy/my report.pdf'");
  });

  it("escapes an embedded single quote", () => {
    // POSIX idiom: close the quote, emit an escaped quote, reopen — 'it'\''s'
    expect(shellQuotePath("/tmp/it's here.txt")).toBe("'/tmp/it'\\''s here.txt'");
  });
});

describe("joinQuotedPaths", () => {
  it("quotes one path and appends a single trailing space", () => {
    expect(joinQuotedPaths(["/a/b.txt"])).toBe("'/a/b.txt' ");
  });

  it("space-separates multiple quoted paths in order with a trailing space", () => {
    expect(joinQuotedPaths(["/a.pdf", "/b.png", "/c.csv"])).toBe(
      "'/a.pdf' '/b.png' '/c.csv' ",
    );
  });

  it("returns an empty string for no paths", () => {
    expect(joinQuotedPaths([])).toBe("");
  });
});
