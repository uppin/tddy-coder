import { useEffect } from "react";
import { PrismLight as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark, oneLight } from "react-syntax-highlighter/dist/esm/styles/prism";
import json from "react-syntax-highlighter/dist/esm/languages/prism/json";
import type { ChatMessage } from "../chat/useAgentChat";

SyntaxHighlighter.registerLanguage("json", json);

/** Pretty-print a raw payload for display: JSON is re-indented to two spaces; a value that isn't
 *  JSON (tool output is often a bare string) is shown verbatim. */
function prettyJson(raw: string): string {
  try {
    return JSON.stringify(JSON.parse(raw), null, 2);
  } catch {
    return raw;
  }
}

/** A single prettified, color-highlighted JSON block (Prism emits `.token` spans). */
function JsonHighlight({ raw }: { raw: string }) {
  const isDark =
    typeof document !== "undefined" && document.documentElement.classList.contains("dark");
  return (
    <div data-testid="agent-activity-json-highlight">
      <SyntaxHighlighter
        language="json"
        style={isDark ? oneDark : oneLight}
        customStyle={{ margin: 0, background: "transparent", fontSize: "0.8125rem" }}
        wrapLongLines
      >
        {prettyJson(raw)}
      </SyntaxHighlighter>
    </div>
  );
}

export interface AgentActivityDetailDialogProps {
  /** The clicked tool-call entry whose input/output are shown. */
  message: ChatMessage;
  onClose: () => void;
}

/**
 * Modal dialog rendering a tool call's `raw_input` and `raw_output` as prettified, color-highlighted
 * JSON. Reuses the modal chrome established by `SessionWorkflowFilesModal` (`fixed inset-0 z-50`,
 * `role="dialog"`, Escape- and backdrop-close, scrollable body).
 *
 * PRD: docs/ft/web/agent-activity-pane.md § Persisted, lazily-counted activity (§3).
 */
export function AgentActivityDetailDialog({ message, onClose }: AgentActivityDetailDialogProps) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  const rawInput = message.rawInput ?? "";
  const rawOutput = message.rawOutput ?? "";

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
      role="presentation"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        data-testid="agent-activity-detail-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="agent-activity-detail-title"
        className="flex max-h-[min(90vh,720px)] w-full max-w-2xl flex-col overflow-hidden rounded-lg border border-border bg-background shadow-lg"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <header className="flex shrink-0 items-center justify-between border-b border-border px-4 py-3">
          <h2 id="agent-activity-detail-title" className="font-mono text-sm font-semibold">
            {message.text}
          </h2>
          <button
            type="button"
            className="rounded-md px-2 py-1 text-sm text-muted-foreground hover:bg-muted"
            onClick={onClose}
            data-testid="agent-activity-detail-close"
          >
            Close
          </button>
        </header>
        <div className="min-h-0 flex-1 space-y-4 overflow-auto p-4">
          <section>
            <h3 className="mb-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
              Input
            </h3>
            <div data-testid="agent-activity-detail-input">
              <JsonHighlight raw={rawInput} />
            </div>
          </section>
          {rawOutput && (
            <section>
              <h3 className="mb-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                Output
              </h3>
              <div data-testid="agent-activity-detail-output">
                <JsonHighlight raw={rawOutput} />
              </div>
            </section>
          )}
        </div>
      </div>
    </div>
  );
}
