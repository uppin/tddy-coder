/**
 * Maps a file path to a Prism language id for syntax highlighting in the Worktree Code pane.
 *
 * Classifies by the basename's lowercased extension. Markdown (`.md`) returns `null` because the
 * preview renders it through the markdown renderer instead. Files with no extension return `null`,
 * so the preview falls back to plain monospace text.
 */

const EXTENSION_TO_PRISM_LANGUAGE: Record<string, string> = {
  rs: "rust",
  ts: "tsx",
  tsx: "tsx",
  js: "jsx",
  jsx: "jsx",
  mjs: "jsx",
  cjs: "jsx",
  py: "python",
  json: "json",
  yaml: "yaml",
  yml: "yaml",
  toml: "toml",
  sh: "bash",
  bash: "bash",
  css: "css",
  html: "markup",
  go: "go",
  rb: "ruby",
  java: "java",
  c: "c",
  h: "c",
  cpp: "cpp",
  cc: "cpp",
  hpp: "cpp",
};

export function codeLanguageForPath(relPath: string): string | null {
  const basename = relPath.split("/").pop() ?? "";
  const dotIndex = basename.lastIndexOf(".");
  if (dotIndex <= 0) {
    return null;
  }
  const ext = basename.slice(dotIndex + 1).toLowerCase();
  return EXTENSION_TO_PRISM_LANGUAGE[ext] ?? null;
}
