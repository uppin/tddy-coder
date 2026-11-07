/**
 * POSIX shell quoting for uploaded file paths that are typed into the terminal.
 *
 * A dropped file's absolute host path is inserted as a shell token, so it must survive word
 * splitting and globbing regardless of spaces or quotes in the path. We use the single-quote
 * idiom: wrap the whole path in `'…'` and rewrite any embedded `'` as `'\''` (close quote, escaped
 * quote, reopen quote).
 *
 * Changeset: `terminal-file-drop-upload`
 * PRD: docs/ft/web/web-terminal.md § File drop upload
 */

/** Quotes a single path as one POSIX shell token. */
export function shellQuotePath(path: string): string {
  return `'${path.replaceAll("'", "'\\''")}'`;
}

/**
 * Quotes each path and joins them space-separated with a single trailing space, emulating a native
 * terminal file drag (each file becomes its own token, cursor left after a separating space). An
 * empty list yields the empty string.
 */
export function joinQuotedPaths(paths: string[]): string {
  if (paths.length === 0) return "";
  return paths.map(shellQuotePath).join(" ") + " ";
}
