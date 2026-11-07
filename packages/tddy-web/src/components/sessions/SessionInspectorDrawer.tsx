import React, { useCallback, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { Room } from "livekit-client";
import type { SessionService, SessionEntry } from "tddy-client-web/gen/session_pb";
import type { ActivityService } from "tddy-client-web/gen/activity_pb";
import type { ExecToolService } from "tddy-client-web/gen/exec_tools_pb";
import type { SessionFilesService } from "tddy-client-web/gen/session_files_pb";
import type { WorktreeService } from "tddy-client-web/gen/worktree_pb";
import { VncService } from "tddy-client-web/gen/vnc_pb";
import { ScreenSharingService } from "tddy-client-web/gen/screen_sharing_pb";
import { ScrollArea } from "tddy-ui-web";
import { cn } from "tddy-ui-web";
import { InspectorTabs, type InspectorTab } from "./InspectorTabs";
import { PARAM_INSPECTOR } from "../../routing/appLocation";
import {
  isDormantInspectorTabName,
  isInspectorTabName,
  isMediaInspectorTabName,
} from "../../routing/appRoutes";
import { useCapabilityAvailability } from "../../hooks/useCapabilityAvailability";
import type { HostConnection } from "tddy-client-web/connections/types";
import { useAppLocation } from "../../routing/useAppLocation";
import { SessionAgentRosterPane } from "./SessionAgentRosterPane";
import { SessionToolsTab } from "./SessionToolsTab";
import { SessionUsageTab } from "./SessionUsageTab";
import { SessionWorktreeTab } from "./SessionWorktreeTab";
import { SessionFilesTab } from "./SessionFilesTab";
import { SessionInspectorActivityTab } from "./SessionInspectorActivityTab";
import { canResumeSession } from "./sessionBaseView";
// (usage stream is owned by SessionUsageTab so it opens only while that tab is mounted)
import { SessionVncTab } from "./SessionVncTab";
import { SessionScreenSharingTab } from "./SessionScreenSharingTab";
import { useHttpClient } from "../../rpc/transportProvider";
import { SessionInspectorDetailsTab, type InspectorTraffic } from "./SessionInspectorDetailsTab";
import { SessionInspectorHeader } from "./SessionInspectorHeader";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type InspectorDrawerState = "closed" | "open" | "expanded";

interface SessionInspectorDrawerProps {
  state: InspectorDrawerState;
  session: SessionEntry | null;
  /** The drawer's full session list — the Agents tab folds this session's subagents out of it. */
  sessions?: ReadonlyArray<SessionEntry>;
  onClose: () => void;
  onExpand: () => void;
  onRestore: () => void;
  onResume: (sessionId: string) => void;
  onDelete: (sessionId: string) => void;
  onTerminate: (sessionId: string) => void;
  client?: Client<typeof SessionService>;
  /** `activity.ActivityService` on the same host as `client` — the Activity tab of a stopped session. */
  activityClient?: Client<typeof ActivityService>;
  /** `exec_tools.ExecToolService` on the same host as `client` — the Tools tab. */
  execToolClient?: Client<typeof ExecToolService>;
  /**
   * The worktree service on the same host as `client` — what the Worktree tab reads its size,
   * diff summary and clear/delete/restore actions through.
   */
  worktreeClient?: Client<typeof WorktreeService>;
  /**
   * The session-files service on the same host as `client` — what the Files tab lists, inserts and
   * deletes a session's uploads through.
   */
  sessionFilesClient?: Client<typeof SessionFilesService>;
  sessionToken?: string;
  /**
   * The connection to the host that owns this session, or `null` when nothing can reach it.
   *
   * The media tabs are gated on this and on nothing else. The host is the upstream fact: a
   * session's own capabilities are derived from whether its attach hint names a room
   * (`capabilitiesForHint`), and whether there is a room at all is decided by how the host is
   * reached — a host reached without LiveKit can never hand out a room-backed session, and so can
   * never serve a VNC or screen-sharing track either.
   *
   * Reading the *session's* connection instead would answer "no media" for a dormant session,
   * which has no connection at all — a refusal to an unanswerable question, which would hide tabs
   * whose capability is in fact present.
   *
   * Required, with no default, for the reason `ParticipantList.connection` and
   * `InspectorTabs.mediaAvailable` are: a default would have to be `null`, `null` means "no media",
   * and so a call site that simply forgot the prop would silently lose the VNC and Screen Sharing
   * tabs — indistinguishable from a host that genuinely cannot carry a track. `null` is still a
   * perfectly good answer; it just has to be given.
   */
  hostConnection: HostConnection | null;
  room?: Room | null;
  /** LiveKit participant identity of the daemon/presenter side, for the token-usage stream.
   *  Selected together with `room`; falls back to `"server"` when not connected over LiveKit. */
  serverIdentity?: string;
  /** Inspector I/O traffic (req 5): byte counters + last-received. Live runtime for active
   *  sessions; daemon-sourced `SessionEntry` fields for inactive / non-LiveKit sessions. */
  traffic?: InspectorTraffic | null;
  /** Lazy builder for a session-scoped `exec_tools.ExecToolService` client (targets the coder
   *  participant for an attached LiveKit session). The Tools tab routes through it when available,
   *  falling back to the daemon `execToolClient` for inactive / non-LiveKit sessions. */
  buildSessionExecToolClient?: () => Client<typeof ExecToolService> | null;
  /** Inserts an uploaded file's host path into the focused session's terminal (Files tab → Insert
   *  / tap). Defaults to a no-op when the host does not route terminal input. */
  onInsertPathIntoTerminal?: (hostPath: string) => void;
  /** Fired when a subagent session's "Switch" is clicked in the Agents tab — the host selects that
   *  session in the drawer (focuses its runtime). No transport of its own. */
  onSwitchPeer?: (sessionId: string) => void;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function SessionInspectorDrawer({
  state,
  session,
  sessions = [],
  onClose,
  onExpand,
  onRestore,
  onResume,
  onDelete,
  onTerminate,
  client,
  activityClient,
  execToolClient,
  worktreeClient,
  sessionFilesClient,
  sessionToken,
  hostConnection,
  room = null,
  serverIdentity = "server",
  traffic = null,
  buildSessionExecToolClient,
  onInsertPathIntoTerminal,
  onSwitchPeer,
}: SessionInspectorDrawerProps) {
  const [pendingDelete, setPendingDelete] = useState(false);
  // The active tab lives in the URL (`?inspector=<tab>`) so a shared link opens on the same tab.
  // An unresolvable value degrades to Details rather than rendering a blank panel; the screen
  // normalises the param itself, so the two never disagree for long.
  const { location, setParams } = useAppLocation();
  const tabParam = location.params[PARAM_INSPECTOR] ?? "";
  const requestedTab: InspectorTab = isInspectorTabName(tabParam) ? tabParam : "details";

  // The one media decision in this drawer: the tab strip, the panel dispatch and the fallback below
  // all read it, so the strip cannot offer a tab the dispatch would refuse to render.
  //
  // Two facts, in the order `useCapabilityAvailability` fixes, and for the same reason the presence
  // surfaces read it: while the common room is joining there is no host connection yet, so the
  // capability alone would render seven tabs on load and nine a second later, reflowing the strip
  // under the operator's cursor. A join in flight, or one that failed, keeps the tabs — the wire
  // that will carry the tracks is the one being established, and a host that never joins a room at
  // all (the desktop build over IPC) reports `idle`, never `connecting`, so nothing appears there
  // only to vanish.
  const mediaAvailable = useCapabilityAvailability(hostConnection, "media") !== "unavailable";

  // A media tab named on a host that cannot carry tracks degrades to Details, the same way an
  // unresolvable tab name does — a deep link, or a URL carried over from a host that did have
  // video, must not land on a panel that can only ever be blank.
  //
  // Unlike an unknown tab name, this one is *not* normalised out of the URL by the screen: media
  // availability is a property of a live connection and can appear later, so the request is kept
  // and honoured the moment the host's wire can serve it.
  //
  // The Activity tab is the recorded transcript of a stopped session, so it exists only while the
  // session is stopped. A live session's transcript is the Agent Activity sidebar's. A request for it
  // on a live session degrades to Details, and is honoured again if the session stops.
  const activityAvailable = canResumeSession(session);
  const tab: InspectorTab =
    (!mediaAvailable && isMediaInspectorTabName(requestedTab)) ||
    (!activityAvailable && isDormantInspectorTabName(requestedTab))
      ? "details"
      : requestedTab;
  const setTab = useCallback(
    (next: InspectorTab) => setParams({ [PARAM_INSPECTOR]: next }),
    [setParams],
  );
  const vncClient = useHttpClient(VncService);
  const screenSharingClient = useHttpClient(ScreenSharingService);

  // Always render in DOM — data-state drives visibility and layout.
  return (
    <div
      data-testid="sessions-inspector-drawer"
      data-state={state}
      // Always an overlay drawer: the pane behind it is never empty (a stopped session shows its
      // Agent tab), so a full-pane inspector would bury the view the operator came for.
      className={cn(
        "flex flex-col h-full border-l border-border bg-background overflow-hidden",
        "absolute top-0 right-0 z-10",
        state === "closed" && "hidden",
        state === "open" && "w-full md:w-[360px]",
        state === "expanded" && "left-0 right-0 w-full",
      )}
    >
      {/* Header */}
      <SessionInspectorHeader state={state} onClose={onClose} onExpand={onExpand} onRestore={onRestore} />

      {/* Tab strip — only when a session is selected */}
      {session && (
        <InspectorTabs
          value={tab}
          onChange={setTab}
          mediaAvailable={mediaAvailable}
          activityAvailable={activityAvailable}
        />
      )}

      {/* Content */}
      {session ? (
        tab === "details" ? (
          <SessionInspectorDetailsTab
            session={session}
            traffic={traffic}
            pendingDelete={pendingDelete}
            onPendingDeleteChange={setPendingDelete}
            onResume={onResume}
            onDelete={onDelete}
            onTerminate={onTerminate}
          />
        ) : tab === "activity" ? (
          // The recorded transcript of a stopped session, scrolled by its own transcript view, so it
          // takes the panel without the ScrollArea the other tabs use.
          <SessionInspectorActivityTab
            session={session}
            sessionToken={sessionToken ?? ""}
            client={activityClient}
          />
        ) : tab === "tools" ? (
          <ScrollArea className="flex-1 min-h-0">
            <SessionToolsTab
              sessionId={session.sessionId}
              onListExecTools={() => {
                const c = buildSessionExecToolClient?.() ?? execToolClient;
                return c
                  ? c
                      .listExecTools({ sessionToken: sessionToken ?? "", daemonInstanceId: "" })
                      .then((r) => r.tools)
                  : Promise.resolve([]);
              }}
              onListSessionToolCalls={() => {
                const c = buildSessionExecToolClient?.() ?? execToolClient;
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
                const c = buildSessionExecToolClient?.() ?? execToolClient;
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
        ) : tab === "agents" ? (
          <ScrollArea className="flex-1 min-h-0">
            {/* The pane derives the roster's address from the session itself (`rosterHalfOf`), so
                the entry and the host it is read on cannot disagree. `client` is this drawer's live
                daemon connection, so its absence is exactly "not connected to that host". */}
            <SessionAgentRosterPane
              session={session}
              sessions={sessions}
              sessionToken={sessionToken ?? ""}
              daemonConnected={client !== undefined}
              onSwitchSubagent={(sessionId) => onSwitchPeer?.(sessionId)}
            />
          </ScrollArea>
        ) : tab === "usage" ? (
          <ScrollArea className="flex-1 min-h-0">
            <SessionUsageTab room={room} serverIdentity={serverIdentity} />
          </ScrollArea>
        ) : tab === "worktree" ? (
          <ScrollArea className="flex-1 min-h-0">
            <SessionWorktreeTab
              client={worktreeClient ?? null}
              sessionToken={sessionToken ?? ""}
              projectId={session.projectId}
              sessionId={session.sessionId}
              repoPath={session.repoPath}
            />
          </ScrollArea>
        ) : tab === "files" ? (
          <ScrollArea className="flex-1 min-h-0">
            <SessionFilesTab
              client={sessionFilesClient ?? null}
              sessionToken={sessionToken ?? ""}
              sessionId={session.sessionId}
              onInsertPath={onInsertPathIntoTerminal ?? (() => undefined)}
              onCloseInspector={onClose}
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
          /* Reached only for `screen-sharing`: every other tab name has an arm above, and both
             media tabs have already been degraded to Details when the host cannot carry tracks —
             which is what keeps this panel, and the `ScreenSharingOverlay` it can open, from
             rendering on a wire with nothing to subscribe to. */
          <ScrollArea className="flex-1 min-h-0">
            <SessionScreenSharingTab
              sessionId={session.sessionId}
              sessionToken={sessionToken ?? ""}
              room={room}
              onListTargets={() =>
                screenSharingClient
                  .listTargets({ sessionToken: sessionToken ?? "", sessionId: session.sessionId })
                  .then((r) => ({
                    vaultLocked: r.vaultLocked,
                    targets: r.targets.map((t) => ({
                      id: t.id,
                      label: t.label,
                      host: t.host,
                      port: t.port,
                      protocol: t.protocol,
                      username: t.username,
                    })),
                  }))
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
