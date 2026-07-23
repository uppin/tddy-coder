import { useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import { toJson } from "@bufbuild/protobuf";
import type { Value } from "@bufbuild/protobuf/wkt";
import { ValueSchema } from "@bufbuild/protobuf/wkt";
import type { AgentActivityRecord } from "../../gen/connection_pb";
import { ConnectionService } from "../../gen/connection_pb";
import { useHttpClient } from "../../rpc/transportProvider";
import { Button } from "../ui/button";
import { useSessionActivity } from "./useSessionActivity";

interface AgentActivityOverlayProps {
  sessionId: string;
  sessionToken: string;
  /** Session type is carried for parity with the rest of the session chrome; the pane itself is
   *  session-type-agnostic (it renders identically for tool, sandbox, claude-cli, … sessions). */
  sessionType?: string;
  /** Explicit client override — session-scoped routing where available. Falls back to the shared
   *  HTTP client from the transport context. */
  client?: Client<typeof ConnectionService>;
}

/**
 * Per-session, top-bar overlay listing the agent's own tool calls (streamed live via
 * `ConnectionService.StreamSessionActivity`). The icon appears only once the session has streamed
 * at least one record; an unread badge flags activity that arrived since the overlay was last
 * opened. Each row expands into a scrollable full-input/output detail dialog.
 *
 * PRD: docs/ft/web/agent-activity-pane.md.
 */
export function AgentActivityOverlay({
  sessionId,
  sessionToken,
  client,
}: AgentActivityOverlayProps) {
  // `useHttpClient` is called unconditionally (hook rules); the explicit prop wins when present.
  const httpClient = useHttpClient(ConnectionService);
  const resolvedClient = client ?? httpClient;

  const { records, hasActivity, unreadCount, markSeen } = useSessionActivity({
    sessionId,
    sessionToken,
    client: resolvedClient,
  });

  const [open, setOpen] = useState(false);
  const [selectedCallId, setSelectedCallId] = useState<string | null>(null);

  // While the overlay is open, freshly-arriving records are considered seen the moment they land.
  useEffect(() => {
    if (open) markSeen();
    // markSeen closes over the current records; re-running when records change keeps the badge
    // clear while the operator is looking at the list.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, records]);

  if (!hasActivity) {
    return null;
  }

  const selectedRecord: AgentActivityRecord | null =
    selectedCallId === null ? null : records.find((r) => r.callId === selectedCallId) ?? null;

  return (
    <>
      <div className="relative inline-flex">
        <Button
          data-testid="agent-activity-button"
          variant="ghost"
          size="icon-sm"
          title="Agent activity"
          onClick={() => {
            setOpen((prev) => {
              if (!prev) markSeen();
              return !prev;
            });
          }}
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <path d="M22 12h-4l-3 9L9 3l-3 9H2" />
          </svg>
        </Button>
        {unreadCount > 0 && (
          <span
            data-testid="agent-activity-unread-badge"
            className="absolute -right-0.5 -top-0.5 flex min-h-4 min-w-4 items-center justify-center rounded-full bg-primary px-1 text-[10px] font-semibold leading-none text-primary-foreground"
          >
            {unreadCount}
          </span>
        )}
      </div>

      {open && (
        <div
          data-testid="agent-activity-overlay"
          className="absolute top-0 right-0 z-10 flex h-full w-full flex-col border-l border-border bg-background md:w-[360px]"
        >
          <div className="flex flex-shrink-0 items-center justify-between border-b border-border px-3 py-2">
            <span className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
              Agent activity
            </span>
            <Button
              data-testid="agent-activity-overlay-close"
              variant="ghost"
              size="icon-sm"
              className="h-6 w-6 p-0"
              onClick={() => setOpen(false)}
              title="Close"
            >
              <svg
                xmlns="http://www.w3.org/2000/svg"
                width="14"
                height="14"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <line x1="18" y1="6" x2="6" y2="18" />
                <line x1="6" y1="6" x2="18" y2="18" />
              </svg>
            </Button>
          </div>

          <div
            data-testid="agent-activity-list"
            className="flex min-h-0 flex-1 flex-col overflow-auto"
          >
            {records.map((record) => (
              <button
                key={record.callId}
                type="button"
                data-testid={`agent-activity-row-${record.callId}`}
                className="flex items-center gap-2 border-b border-border px-3 py-2 text-left text-xs hover:bg-muted"
                onClick={() => setSelectedCallId(record.callId)}
              >
                <span className="truncate font-medium">{record.toolName}</span>
                {record.status === "running" && (
                  <span className="text-muted-foreground">[running]</span>
                )}
                {record.status === "error" && (
                  <span className="text-destructive">[error]</span>
                )}
              </button>
            ))}
          </div>
        </div>
      )}

      {selectedRecord && (
        <AgentActivityDetailDialog
          record={selectedRecord}
          onClose={() => setSelectedCallId(null)}
        />
      )}
    </>
  );
}

/**
 * Render a `google.protobuf.Value` tool input/output as pretty-printed text. An unset value (the
 * "running" row's output, or an absent input) renders as an empty string. A bare-string value
 * renders as the string itself; objects/arrays/scalars pretty-print as indented JSON.
 */
function formatValue(value: Value | undefined): string {
  if (value === undefined) return "";
  const json = toJson(ValueSchema, value);
  if (typeof json === "string") return json;
  return JSON.stringify(json, null, 2);
}

/** Full-input/output detail dialog for one tool call. Escape and backdrop click both dismiss it. */
function AgentActivityDetailDialog({
  record,
  onClose,
}: {
  record: AgentActivityRecord;
  onClose: () => void;
}) {
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

  return (
    <div
      // Inline layout mirrors the Tailwind classes so the full-viewport, dialog-centered geometry
      // holds even where the utility stylesheet is absent — the centered dialog leaves the backdrop
      // corners clear so a corner click lands on the backdrop (dismiss), never the dialog box.
      style={{
        position: "fixed",
        inset: 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
      }}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
      role="presentation"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        data-testid="agent-activity-detail-dialog"
        role="dialog"
        aria-modal="true"
        aria-label={record.toolName}
        className="flex max-h-[min(90vh,720px)] w-full max-w-3xl flex-col overflow-hidden rounded-lg border border-border bg-background shadow-lg"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <header className="flex shrink-0 items-center justify-between border-b border-border px-4 py-3">
          <h2 className="text-lg font-semibold">{record.toolName}</h2>
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
          <section className="space-y-1">
            <h3 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
              Input
            </h3>
            <pre
              data-testid="agent-activity-detail-input"
              className="whitespace-pre-wrap break-all rounded-md bg-muted p-2 font-mono text-xs"
            >
              {formatValue(record.input)}
            </pre>
          </section>
          <section className="space-y-1">
            <h3 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
              Output
            </h3>
            <pre
              data-testid="agent-activity-detail-output"
              className="whitespace-pre-wrap break-all rounded-md bg-muted p-2 font-mono text-xs"
            >
              {formatValue(record.result)}
            </pre>
          </section>
        </div>
      </div>
    </div>
  );
}
