import React, { useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { ConnectionService, SessionEntry } from "../../gen/connection_pb";
import { Button } from "../ui/button";
import { ScrollArea } from "../ui/scroll-area";
import { cn } from "../../lib/utils";
import { InspectorTabs, type InspectorTab } from "./InspectorTabs";
import { SessionToolsTab } from "./SessionToolsTab";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type InspectorDrawerState = "closed" | "open" | "expanded";

interface SessionInspectorDrawerProps {
  state: InspectorDrawerState;
  session: SessionEntry | null;
  onClose: () => void;
  onExpand: () => void;
  onRestore: () => void;
  onResume: (sessionId: string) => void;
  onDelete: (sessionId: string) => void;
  onTerminate: (sessionId: string) => void;
  client?: Client<typeof ConnectionService>;
  sessionToken?: string;
}

// ---------------------------------------------------------------------------
// Metadata row helper
// ---------------------------------------------------------------------------

function MetaRow({ label, value }: { label: string; value: string | number | undefined | null }) {
  if (value === undefined || value === null || value === "" || value === 0) return null;
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-xs text-muted-foreground">{label}</span>
      <span className="text-xs break-all">{String(value)}</span>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function SessionInspectorDrawer({
  state,
  session,
  onClose,
  onExpand,
  onRestore,
  onResume,
  onDelete,
  onTerminate,
  client,
  sessionToken,
}: SessionInspectorDrawerProps) {
  const [pendingDelete, setPendingDelete] = useState(false);
  const [tab, setTab] = useState<InspectorTab>("details");

  // Always render in DOM — data-state drives visibility and layout.
  return (
    <div
      data-testid="sessions-inspector-drawer"
      data-state={state}
      className={cn(
        "flex flex-col h-full border-l border-border bg-background overflow-hidden",
        "absolute top-0 right-0 z-10",
        state === "closed" && "hidden",
        state === "open" && "w-[360px]",
        state === "expanded" && "left-0 right-0 w-full",
      )}
    >
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-border flex-shrink-0">
        <span className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
          Inspector
        </span>
        <div className="flex items-center gap-1">
          {state === "open" && (
            <Button
              data-testid="sessions-inspector-expand"
              variant="ghost"
              size="sm"
              className="h-6 w-6 p-0"
              onClick={onExpand}
              title="Expand"
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
                <polyline points="15 3 21 3 21 9" />
                <polyline points="9 21 3 21 3 15" />
                <line x1="21" y1="3" x2="14" y2="10" />
                <line x1="3" y1="21" x2="10" y2="14" />
              </svg>
            </Button>
          )}
          {state === "expanded" && (
            <Button
              data-testid="sessions-inspector-restore"
              variant="ghost"
              size="sm"
              className="h-6 w-6 p-0"
              onClick={onRestore}
              title="Restore"
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
                <polyline points="4 14 10 14 10 20" />
                <polyline points="20 10 14 10 14 4" />
                <line x1="10" y1="14" x2="3" y2="21" />
                <line x1="21" y1="3" x2="14" y2="10" />
              </svg>
            </Button>
          )}
          <Button
            data-testid="sessions-inspector-close"
            variant="ghost"
            size="sm"
            className="h-6 w-6 p-0"
            onClick={onClose}
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
      </div>

      {/* Tab strip — only when a session is selected */}
      {session && <InspectorTabs value={tab} onChange={setTab} />}

      {/* Content */}
      {session ? (
        tab === "details" ? (
          <ScrollArea className="flex-1 min-h-0">
            <div className="px-3 py-3 flex flex-col gap-4">
              {/* Metadata */}
              <div
                data-testid="sessions-inspector-metadata"
                className="flex flex-col gap-2"
              >
                <MetaRow label="Goal" value={session.workflowGoal} />
                <MetaRow label="Status" value={session.status} />
                <MetaRow label="Repo" value={session.repoPath} />
                <MetaRow label="Session ID" value={session.sessionId} />
                <MetaRow label="PID" value={session.pid} />
                <MetaRow label="Workflow state" value={session.workflowState} />
                <MetaRow label="Activity status" value={session.activityStatus} />
                <MetaRow label="Agent" value={session.agent} />
                <MetaRow label="Model" value={session.model} />
                <MetaRow label="Created" value={session.createdAt} />
                <MetaRow label="Updated" value={session.updatedAt} />
                <MetaRow label="Elapsed" value={session.elapsedDisplay} />
                <MetaRow label="Tool" value={session.tool} />
                <MetaRow label="Session type" value={session.sessionType} />
                <MetaRow label="LiveKit room" value={session.livekitRoom} />
                <MetaRow label="Previous session" value={session.previousSessionId} />
              </div>

              {/* Controls */}
              <div className="flex flex-col gap-2">
                {!session.isActive && (
                  <Button
                    data-testid={`sessions-inspector-resume-${session.sessionId}`}
                    size="sm"
                    onClick={() => {
                      setPendingDelete(false);
                      onResume(session.sessionId);
                    }}
                  >
                    Resume
                  </Button>
                )}

                {!session.isActive && !pendingDelete && (
                  <Button
                    data-testid={`sessions-inspector-delete-${session.sessionId}`}
                    variant="destructive"
                    size="sm"
                    onClick={() => setPendingDelete(true)}
                  >
                    Delete
                  </Button>
                )}

                {!session.isActive && pendingDelete && (
                  <Button
                    data-testid={`sessions-inspector-delete-confirm-${session.sessionId}`}
                    variant="destructive"
                    size="sm"
                    onClick={() => {
                      setPendingDelete(false);
                      onDelete(session.sessionId);
                    }}
                  >
                    Confirm delete
                  </Button>
                )}

                {session.isActive && (
                  <Button
                    data-testid={`sessions-inspector-terminate-${session.sessionId}`}
                    variant="destructive"
                    size="sm"
                    onClick={() => onTerminate(session.sessionId)}
                  >
                    Terminate
                  </Button>
                )}
              </div>
            </div>
          </ScrollArea>
        ) : (
          <ScrollArea className="flex-1 min-h-0">
            <SessionToolsTab
              sessionId={session.sessionId}
              onListExecTools={() =>
                client
                  ? client
                      .listExecTools({ sessionToken: sessionToken ?? "", daemonInstanceId: "" })
                      .then((r) => r.tools)
                  : Promise.resolve([])
              }
              onListSessionToolCalls={() =>
                client
                  ? client
                      .listSessionToolCalls({
                        sessionToken: sessionToken ?? "",
                        sessionId: session.sessionId,
                        daemonInstanceId: "",
                      })
                      .then((r) => r.toolCalls)
                  : Promise.resolve([])
              }
              onExecuteTool={({ toolName, argsJson }) =>
                client
                  ? client.executeTool({
                      sessionToken: sessionToken ?? "",
                      sessionId: session.sessionId,
                      toolName,
                      argsJson,
                      daemonInstanceId: "",
                    })
                  : Promise.resolve({ resultJson: "", isError: true, errorMessage: "no client" })
              }
            />
          </ScrollArea>
        )
      ) : (
        <div className="flex items-center justify-center flex-1 text-sm text-muted-foreground">
          No session selected
        </div>
      )}
    </div>
  );
}
