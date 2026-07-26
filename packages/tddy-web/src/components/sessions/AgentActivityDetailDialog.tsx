import { useEffect } from "react";
import type { Client } from "@connectrpc/connect";
import { PrismLight as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark, oneLight } from "react-syntax-highlighter/dist/esm/styles/prism";
import json from "react-syntax-highlighter/dist/esm/languages/prism/json";
import type { ConnectionService } from "../../gen/connection_pb";
import type { ChatMessage } from "../chat/useAgentChat";
import { useAcpToolCallDetail, type ToolCallDetailErrorKind } from "../chat/useAcpToolCallDetail";

SyntaxHighlighter.registerLanguage("json", json);

/** Shown under the Output heading when the lookup resolved without an output body. A running call may
 *  still produce one; a settled call never will, so the two are worded apart rather than both
 *  claiming the call is in progress. */
const NO_OUTPUT_YET = "No output yet — tool call still running.";
const NO_OUTPUT_RECORDED = "No output recorded for this tool call.";

/** Shown under the Input heading when the lookup resolved without an input body (`raw_input` is
 *  `optional` on the response, so its absence is a legitimate success). Stated in the same style as
 *  the no-output note instead of rendering an empty highlighted block, which would read as "the input
 *  was empty" — a claim the response does not make. */
const NO_INPUT_RECORDED = "No input recorded for this tool call.";

/** What the operator is told for each way a body lookup can end unanswered. `missingId` is not a
 *  transport outcome at all — the entry carried no `tool_call_id`, so nothing was ever asked — and must
 *  not borrow the `notFound` wording, which asserts the host looked and found no such call. */
const DETAIL_ERROR_TEXT: Record<ToolCallDetailErrorKind, string> = {
  missingId: "This activity entry has no tool call id, so its input and output are unavailable.",
  notFound: "This tool call is not in the session transcript.",
  failed: "Could not load tool call details.",
};

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

/** Placeholder standing in for a JSON block while its body lookup is in flight — a blank dialog would
 *  be indistinguishable from a call with no bodies at all. Inlined rather than promoted to a shared
 *  primitive: this is the only skeleton in tddy-web today. */
function BodySkeleton() {
  return (
    <div
      data-testid="agent-activity-detail-skeleton"
      role="status"
      aria-label="Loading tool call details"
      className="animate-pulse space-y-2 py-1"
    >
      <div className="h-3 w-3/4 rounded bg-muted" />
      <div className="h-3 w-1/2 rounded bg-muted" />
      <div className="h-3 w-2/3 rounded bg-muted" />
    </div>
  );
}

export interface AgentActivityDetailDialogProps {
  /** The clicked tool-call entry, whose `toolCallId` names the call to look up and whose
   *  `toolStatus` decides whether the resolved bodies are final enough to cache. */
  message: ChatMessage;
  sessionId: string;
  sessionToken: string;
  /** The same client the transcript stream uses, so the body lookup is routed identically. */
  client: Client<typeof ConnectionService>;
  onClose: () => void;
}

/**
 * Modal dialog rendering a tool call's `raw_input` and `raw_output` as prettified, color-highlighted
 * JSON. Reuses the modal chrome established by `SessionWorkflowFilesModal` (`fixed inset-0 z-50`,
 * `role="dialog"`, Escape- and backdrop-close, scrollable body).
 *
 * The bodies are not on the transcript entry — `StreamAcpReplay` strips them from every frame — so
 * they are fetched on open by {@link useAcpToolCallDetail}, giving the dialog four states: a skeleton
 * while the lookup is in flight, the JSON blocks once it resolves, an explicit note when either body
 * is absent (a running call has no output yet; `raw_input` is optional too), and an inline
 * `role="alert"` when the bodies cannot be shown at all — the lookup failed, the host knows no such
 * call, or the entry never carried a `tool_call_id` to look up (see {@link DETAIL_ERROR_TEXT}).
 *
 * PRD: docs/ft/web/agent-activity-pane.md § Persisted, lazily-counted activity (§3).
 * Feature doc: docs/ft/web/agent-activity-pane.md#rendering-an-unanswered-lookup-updated-2026-07-26
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

  const isRunning = message.toolStatus === "running";
  const detail = useAcpToolCallDetail({
    sessionId,
    sessionToken,
    client,
    toolCallId: message.toolCallId ?? "",
    // A running call's output can still arrive, so its bodies are re-fetched on every open instead of
    // being cached partial for the rest of the session.
    cacheable: !isRunning,
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
          {detail.status === "error" ? (
            <p
              data-testid="agent-activity-detail-error"
              role="alert"
              className="text-xs text-destructive"
            >
              {DETAIL_ERROR_TEXT[detail.kind]}
            </p>
          ) : (
            <>
              <section>
                <h3 className="mb-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                  Input
                </h3>
                {detail.status === "loading" ? (
                  <BodySkeleton />
                ) : detail.rawInput ? (
                  <div data-testid="agent-activity-detail-input">
                    <JsonHighlight raw={detail.rawInput} />
                  </div>
                ) : (
                  <p
                    data-testid="agent-activity-detail-no-input"
                    className="text-xs text-muted-foreground"
                  >
                    {NO_INPUT_RECORDED}
                  </p>
                )}
              </section>
              <section>
                <h3 className="mb-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                  Output
                </h3>
                {detail.status === "loading" ? (
                  <BodySkeleton />
                ) : detail.rawOutput ? (
                  <div data-testid="agent-activity-detail-output">
                    <JsonHighlight raw={detail.rawOutput} />
                  </div>
                ) : (
                  <p
                    data-testid="agent-activity-detail-no-output"
                    className="text-xs text-muted-foreground"
                  >
                    {isRunning ? NO_OUTPUT_YET : NO_OUTPUT_RECORDED}
                  </p>
                )}
              </section>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
