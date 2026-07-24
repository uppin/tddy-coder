import React, { useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { Room } from "livekit-client";
import type { ConnectionService, SessionEntry } from "../../gen/connection_pb";
import { VncService } from "../../gen/vnc_pb";
import { ScreenSharingService } from "../../gen/screen_sharing_pb";
import { Button } from "../ui/button";
import { ScrollArea } from "../ui/scroll-area";
import { cn } from "../../lib/utils";
import { InspectorTabs, type InspectorTab } from "./InspectorTabs";
import { SessionToolsTab } from "./SessionToolsTab";
import { SessionUsageTab } from "./SessionUsageTab";
import { SessionWorktreeTab } from "./SessionWorktreeTab";
// (usage stream is owned by SessionUsageTab so it opens only while that tab is mounted)
import { SessionVncTab } from "./SessionVncTab";
import { SessionScreenSharingTab } from "./SessionScreenSharingTab";
import { useHttpClient } from "../../rpc/transportProvider";
import { formatLastDataReceived } from "./lastDataReceivedFormat";
import { formatBytes } from "./formatTraffic";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type InspectorDrawerState = "closed" | "open" | "expanded";

interface SessionInspectorDrawerProps {
  state: InspectorDrawerState;
  session: SessionEntry | null;
  /** When true, the inspector renders as the full main pane (docked) rather than an overlay
   *  drawer — used for disconnected sessions. */
  docked?: boolean;
  onClose: () => void;
  onExpand: () => void;
  onRestore: () => void;
  onResume: (sessionId: string) => void;
  onDelete: (sessionId: string) => void;
  onTerminate: (sessionId: string) => void;
  client?: Client<typeof ConnectionService>;
  sessionToken?: string;
  room?: Room | null;
  /** LiveKit participant identity of the daemon/presenter side, for the token-usage stream.
   *  Selected together with `room`; falls back to `"server"` when not connected over LiveKit. */
  serverIdentity?: string;
  /** Inspector I/O traffic (req 5): byte counters + last-received. Live runtime for active
   *  sessions; daemon-sourced `SessionEntry` fields for inactive / non-LiveKit sessions. */
  traffic?: { bytesIn: number; bytesOut: number; lastDataReceivedAt: number | null } | null;
  /** Lazy builder for a session-scoped `ConnectionService` client (targets the coder participant
   *  for an attached LiveKit session). The Tools tab routes `ListExecTools` / `ListSessionToolCalls`
   *  / `ExecuteTool` through it when available, falling back to the daemon `client` for inactive /
   *  non-LiveKit sessions. */
  buildSessionClient?: () => Client<typeof ConnectionService> | null;
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
  docked = false,
  onClose,
  onExpand,
  onRestore,
  onResume,
  onDelete,
  onTerminate,
  client,
  sessionToken,
  room = null,
  serverIdentity = "server",
  traffic = null,
  buildSessionClient,
}: SessionInspectorDrawerProps) {
  const [pendingDelete, setPendingDelete] = useState(false);
  const [tab, setTab] = useState<InspectorTab>("details");
  const vncClient = useHttpClient(VncService);
  const screenSharingClient = useHttpClient(ScreenSharingService);

  // Always render in DOM — data-state drives visibility and layout.
  return (
    <div
      data-testid="sessions-inspector-drawer"
      data-state={state}
      data-docked={docked ? "true" : "false"}
      className={cn(
        "flex flex-col h-full border-l border-border bg-background overflow-hidden",
        "absolute top-0 right-0 z-10",
        state === "closed" && "hidden",
        // Docked (disconnected session): the inspector IS the main pane — full-pane footprint for
        // both open and expanded, layered opaque over the still-mounted runtime layer behind it.
        docked && state !== "closed" && "left-0 right-0 w-full",
        !docked && state === "open" && "w-full md:w-[360px]",
        !docked && state === "expanded" && "left-0 right-0 w-full",
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

              {/* I/O traffic (req 5 dual source) — bytes in / bytes out / last data received. */}
              <div className="flex flex-col gap-1">
                <span
                  data-testid="sessions-inspector-bytes-in"
                  className="text-xs"
                >
                  {formatBytes(traffic?.bytesIn ?? 0)}
                </span>
                <span
                  data-testid="sessions-inspector-bytes-out"
                  className="text-xs"
                >
                  {formatBytes(traffic?.bytesOut ?? 0)}
                </span>
                <span
                  data-testid="sessions-inspector-last-data-received"
                  className="text-xs"
                >
                  {formatLastDataReceived(traffic?.lastDataReceivedAt ?? null, Date.now())}
                </span>
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
        ) : tab === "tools" ? (
          <ScrollArea className="flex-1 min-h-0">
            <SessionToolsTab
              sessionId={session.sessionId}
              onListExecTools={() => {
                const c = buildSessionClient?.() ?? client;
                return c
                  ? c
                      .listExecTools({ sessionToken: sessionToken ?? "", daemonInstanceId: "" })
                      .then((r) => r.tools)
                  : Promise.resolve([]);
              }}
              onListSessionToolCalls={() => {
                const c = buildSessionClient?.() ?? client;
                return c
                  ? c
                      .listSessionToolCalls({
                        sessionToken: sessionToken ?? "",
                        sessionId: session.sessionId,
                        daemonInstanceId: "",
                      })
                      .then((r) => r.toolCalls)
                  : Promise.resolve([]);
              }}
              onExecuteTool={({ toolName, argsJson }) => {
                const c = buildSessionClient?.() ?? client;
                return c
                  ? c.executeTool({
                      sessionToken: sessionToken ?? "",
                      sessionId: session.sessionId,
                      toolName,
                      argsJson,
                      daemonInstanceId: "",
                    })
                  : Promise.resolve({ resultJson: "", isError: true, errorMessage: "no client" });
              }}
            />
          </ScrollArea>
        ) : tab === "usage" ? (
          <ScrollArea className="flex-1 min-h-0">
            <SessionUsageTab room={room} serverIdentity={serverIdentity} />
          </ScrollArea>
        ) : tab === "worktree" ? (
          <ScrollArea className="flex-1 min-h-0">
            <SessionWorktreeTab
              client={client ?? null}
              sessionToken={sessionToken ?? ""}
              projectId={session.projectId}
              sessionId={session.sessionId}
              repoPath={session.repoPath}
            />
          </ScrollArea>
        ) : tab === "vnc" ? (
          <ScrollArea className="flex-1 min-h-0">
            <SessionVncTab
              sessionId={session.sessionId}
              sessionToken={sessionToken ?? ""}
              room={room}
              onListVncTargets={() =>
                vncClient
                  .listVncTargets({ sessionToken: sessionToken ?? "", sessionId: session.sessionId })
                  .then((r) => r.targets.map((t) => ({ id: t.id, label: t.label, host: t.host, port: t.port })))
              }
              onAddVncTarget={(req) =>
                vncClient
                  .addVncTarget({
                    sessionToken: sessionToken ?? "",
                    sessionId: session.sessionId,
                    label: req.label,
                    host: req.host,
                    port: req.port,
                    password: req.password,
                  })
                  .then((r) => ({
                    id: r.target?.id ?? "",
                    label: r.target?.label ?? "",
                    host: r.target?.host ?? "",
                    port: r.target?.port ?? 0,
                  }))
              }
              onRemoveVncTarget={(targetId) =>
                vncClient
                  .removeVncTarget({ sessionToken: sessionToken ?? "", sessionId: session.sessionId, targetId })
                  .then(() => undefined)
              }
              onUnlockVncVault={(passphrase) =>
                vncClient
                  .unlockVncVault({ sessionToken: sessionToken ?? "", sessionId: session.sessionId, passphrase })
                  .then(() => undefined)
              }
              onStartVncStream={(targetId) =>
                vncClient
                  .startVncStream({ sessionToken: sessionToken ?? "", sessionId: session.sessionId, targetId })
                  .then((r) => ({
                    livekitRoom: r.livekitRoom,
                    livekitUrl: r.livekitUrl,
                    bridgeIdentity: r.bridgeIdentity,
                    trackName: r.trackName,
                    width: r.width,
                    height: r.height,
                  }))
              }
              onStopVncStream={(targetId) =>
                vncClient
                  .stopVncStream({ sessionToken: sessionToken ?? "", sessionId: session.sessionId, targetId })
                  .then(() => undefined)
              }
            />
          </ScrollArea>
        ) : (
          <ScrollArea className="flex-1 min-h-0">
            <SessionScreenSharingTab
              sessionId={session.sessionId}
              sessionToken={sessionToken ?? ""}
              room={room}
              onListTargets={() =>
                screenSharingClient
                  .listTargets({ sessionToken: sessionToken ?? "", sessionId: session.sessionId })
                  .then((r) =>
                    r.targets.map((t) => ({
                      id: t.id,
                      label: t.label,
                      host: t.host,
                      port: t.port,
                      protocol: t.protocol,
                      username: t.username,
                    }))
                  )
              }
              onAddTarget={(req) =>
                screenSharingClient
                  .addTarget({
                    sessionToken: sessionToken ?? "",
                    sessionId: session.sessionId,
                    label: req.label,
                    host: req.host,
                    port: req.port,
                    username: req.username,
                    password: req.password,
                    protocol: req.protocol,
                  })
                  .then((r) => ({
                    id: r.target?.id ?? "",
                    label: r.target?.label ?? "",
                    host: r.target?.host ?? "",
                    port: r.target?.port ?? 0,
                    protocol: r.target?.protocol ?? 0,
                    username: r.target?.username ?? "",
                  }))
              }
              onRemoveTarget={(targetId) =>
                screenSharingClient
                  .removeTarget({ sessionToken: sessionToken ?? "", sessionId: session.sessionId, targetId })
                  .then(() => undefined)
              }
              onUnlockVault={(passphrase) =>
                screenSharingClient
                  .unlockVault({ sessionToken: sessionToken ?? "", sessionId: session.sessionId, passphrase })
                  .then(() => undefined)
              }
              onStartStream={(targetId) =>
                screenSharingClient
                  .startStream({ sessionToken: sessionToken ?? "", sessionId: session.sessionId, targetId })
                  .then((r) => ({
                    livekitRoom: r.livekitRoom,
                    livekitUrl: r.livekitUrl,
                    bridgeIdentity: r.bridgeIdentity,
                    trackName: r.trackName,
                    width: r.width,
                    height: r.height,
                  }))
              }
              onStopStream={(targetId) =>
                screenSharingClient
                  .stopStream({ sessionToken: sessionToken ?? "", sessionId: session.sessionId, targetId })
                  .then(() => undefined)
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
