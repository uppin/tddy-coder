import { useEffect } from "react";
import { PrismLight as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark, oneLight } from "react-syntax-highlighter/dist/esm/styles/prism";
import json from "react-syntax-highlighter/dist/esm/languages/prism/json";
import type { Client } from "@connectrpc/connect";
import type { ConnectionService } from "../../gen/connection_pb";
import type { ChatMessage } from "../chat/useAgentChat";
import { useToolCallDetail } from "./useToolCallDetail";

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
  /** The clicked tool-call entry whose input/output are shown. Its `toolCallId` drives the on-demand
   *  body fetch. */
  message: ChatMessage;
  /** The session the tool call belongs to — part of the `GetAcpToolCallDetail` lookup key. */
  sessionId: string;
  /** The session token authorizing the lookup. */
  sessionToken: string;
  /** The resolved RPC client the lookup runs over. */
  client: Client<typeof ConnectionService>;
  onClose: () => void;
}

/**
 * Modal dialog rendering a tool call's `raw_input` and `raw_output` as prettified, color-highlighted
 * JSON. The body is not carried on the stream frame (PR #345 strips it); it is fetched on demand via
 * `GetAcpToolCallDetail` (see {@link useToolCallDetail}) — the dialog shows a loading state while the
 * fetch is in flight and an error state if it fails. Reuses the modal chrome established by
 * `SessionWorkflowFilesModal` (`fixed inset-0 z-50`, `role="dialog"`, Escape- and backdrop-close,
 * scrollable body).
 *
 * PRD: docs/ft/web/agent-activity-pane.md § 4 Lazy tool bodies — fetch on click.
 */
export function AgentActivityDetailDialog({
  message,
  sessionId,
  sessionToken,
  client,
  onClose,
}: AgentActivityDetailDialogProps) {
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

  const body = useToolCallDetail({
    sessionId,
    callId: message.toolCallId ?? "",
    sessionToken,
    client,
  });

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
          {body.status === "loading" && (
            <div
              data-testid="agent-activity-detail-loading"
              className="text-sm text-muted-foreground"
            >
              Loading…
            </div>
          )}
          {body.status === "error" && (
            <div data-testid="agent-activity-detail-error" className="text-sm text-destructive">
              {body.error}
            </div>
          )}
          {body.status === "loaded" && (
            <>
              <section>
                <h3 className="mb-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                  Input
                </h3>
                <div data-testid="agent-activity-detail-input">
                  <JsonHighlight raw={body.rawInput ?? ""} />
                </div>
              </section>
              {body.rawOutput && (
                <section>
                  <h3 className="mb-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                    Output
                  </h3>
                  <div data-testid="agent-activity-detail-output">
                    <JsonHighlight raw={body.rawOutput} />
                  </div>
                </section>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}
