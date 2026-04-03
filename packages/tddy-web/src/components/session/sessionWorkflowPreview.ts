/**
 * Classifies a workflow file basename for preview routing (YAML vs Markdown vs plain).
 * Aligns with server-side allowlisted workflow filenames.
 */
export type WorkflowPreviewKind = "markdown" | "yaml" | "plain";

export function workflowPreviewKind(basename: string): WorkflowPreviewKind {
  const lower = basename.toLowerCase();
  if (lower.endsWith(".md")) {
    return "markdown";
  }
  if (lower.endsWith(".yaml") || lower.endsWith(".yml")) {
    return "yaml";
  }
  return "plain";
}
