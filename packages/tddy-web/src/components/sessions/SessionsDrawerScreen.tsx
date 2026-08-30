import React, { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { create } from "@bufbuild/protobuf";
import { createClient, type Client } from "@connectrpc/connect";
import type { Room } from "livekit-client";
import {
  ConnectionService,
  SessionEntrySchema,
  type SessionEntry,
  type ProjectEntry,
} from "../../gen/connection_pb";
import { TokenService } from "../../gen/token_pb";
import {
  useHttpClient,
  useLiveKitTransportFactory,
  useLiveKitTransportFactoryIsOverridden,
} from "../../rpc/transportProvider";
import { useDaemonClient, useDaemonClientFor, useDaemons, useSelectedDaemon } from "../../rpc/selectedDaemon";
import { UploadProgressProvider } from "../../rpc/uploadProgress";
import { daemonRpcIdentity } from "../../lib/participantRole";
import { owningHostForSession } from "../../utils/crossHostSessions";
import { useRoomParticipants } from "../../hooks/useRoomParticipants";
import { requestSessionsRefresh } from "../../lib/sessionsRefreshBridge";
import { useSessionManager } from "./sessionManager";
import {
  attachActionForSnapshot,
  claimAfterFeedEnd,
  type AttachClaim,
} from "./attachClaim";
import {
  SessionRuntimeRegistry,
  makeByteTap,
  type ByteDelta,
  type SessionRuntimeConnection,
} from "./sessionRuntimeRegistry";
import { useSessionClientCache } from "./sessionClientCache";
import { sessionNotificationRegistry } from "./sessionNotificationRegistry";
import { useSessionNotifications } from "../../rpc/useSessionNotifications";
import { useAuthContext } from "../../hooks/authProvider";
import { AppShell } from "../shell/AppShell";
import { Button } from "../ui/button";
import { TooltipProvider } from "../ui/tooltip";
import { SessionDrawer } from "./SessionDrawer";
import { SessionMainPane } from "./SessionMainPane";
import { HostStatsFooter } from "./HostStatsFooter";
import { useSessionAttachment, type SessionAttachmentState } from "./useSessionAttachment";
import { nextInspectorState } from "./inspectorState";
import {
  sessionsDrawerPathForSession,
  parseSessionsDrawerSessionId,
  isSessionsNewPath,
  isInspectorTabName,
  SESSIONS_DRAWER_ROUTE,
  SESSIONS_NEW_ROUTE,
  type InspectorTabName,
} from "../../routing/appRoutes";
import { PARAM_CODE, PARAM_FULL, PARAM_INSPECTOR } from "../../routing/appLocation";
import { useAppLocation } from "../../routing/useAppLocation";
import { Signal } from "../../gen/connection_pb";
import type { InspectorDrawerState } from "./SessionInspectorDrawer";
import { detectIsMobile, useIsMobile } from "../../hooks/useIsMobile";
import { resolveShortcutsForSession } from "../../lib/toolShortcuts";
import { joinQuotedPaths } from "../../lib/shellQuote";
import { isCliTerminalSession } from "../../constants/claudeCliModels";
import { PanelLeftOpen } from "lucide-react";

// ---------------------------------------------------------------------------
// Screen
// ---------------------------------------------------------------------------

export function SessionsDrawerScreen({
  // Optional so isolated component tests can mount the screen without a router; production
  // (index.tsx) always wires the hash-router navigate.
  onNavigate = () => {},
}: {
  onNavigate?: (path: string) => void;
}) {
  const { sessionToken: authSessionToken } = useAuthContext();
  const sessionToken = authSessionToken ?? "";

  // ConnectionService is daemon-level RPC — routed over the shared common-room LiveKit
  // connection to whichever daemon is currently selected (see `SelectedDaemonProvider`).
  // `null` until a daemon is selected / the room is connected; every call site below guards.
  // The selected-daemon `client` still owns the CREATE flow (a new session is created on the
  // selected host); cross-host interaction routes through `activeClient` (computed below).
  const client = useDaemonClient(ConnectionService);

  // One daemon-level notification feed for the whole drawer, however many rows it has (NFR1). The
  // hook's only output is the write into `sessionNotificationRegistry`, which each row reads for
  // itself — hence the bare call.
  useSessionNotifications();

  const { room, selectedInstanceId } = useSelectedDaemon();
  const daemons = useDaemons();
  const liveKitFactory = useLiveKitTransportFactory();
  // TokenService issues this session's own browser LiveKit-join token — it must stay HTTP to the
  // serving daemon (you cannot fetch a LiveKit-join token *over* LiveKit), per the PRD's bootstrap
  // exception. Do not migrate this to useDaemonClient.
  const tokenClient = useHttpClient(TokenService);

  // Address any daemon's ConnectionService directly (`daemon-{instanceId}`) over the shared
  // common-room connection. Used to connect to a cross-host row's owning daemon at click time, when
  // the owner is known but the selected session (and thus `activeClient`) hasn't updated yet.
  const clientForHost = useCallback(
    (instanceId: string): Client<typeof ConnectionService> | null =>
      room && instanceId
        ? createClient(ConnectionService, liveKitFactory(room, daemonRpcIdentity(instanceId)))
        : null,
    [room, liveKitFactory],
  );

  // Selection is derived from the URL, not held in state: that is what makes Back, Forward, a
  // pasted link and a reload all move the screen through the same code path.
  const { location, navigate, setParams } = useAppLocation();
  const selectedSessionId = parseSessionsDrawerSessionId(location.path);
  const mode: "list" | "creating" = isSessionsNewPath(location.path) ? "creating" : "list";

  // Inspector state, likewise: the tab's presence in the URL *is* "the inspector is open".
  const inspectorTabParam = location.params[PARAM_INSPECTOR] ?? "";
  const inspectorTab: InspectorTabName = isInspectorTabName(inspectorTabParam)
    ? inspectorTabParam
    : "details";
  const inspectorState: InspectorDrawerState =
    inspectorTabParam === ""
      ? "closed"
      : location.params[PARAM_FULL] === "1"
        ? "expanded"
        : "open";

  /**
   * Write an inspector state into the URL. `replace` for the transitions the screen makes on the
   * operator's behalf (the closed default on selection, the open on an attach error) so Back does
   * not step through states nobody chose; a plain push for a click.
   */
  const applyInspectorState = useCallback(
    (next: InspectorDrawerState, options?: { replace?: boolean; tab?: InspectorTabName }) => {
      const tab = options?.tab ?? inspectorTab;
      setParams(
        next === "closed"
          ? { [PARAM_INSPECTOR]: null, [PARAM_FULL]: null }
          : { [PARAM_INSPECTOR]: tab, [PARAM_FULL]: next === "expanded" ? "1" : null },
        options,
      );
    },
    [inspectorTab, setParams],
  );

  // A tab name the inspector does not have would render a blank panel — normalise it away.
  useEffect(() => {
    if (inspectorTabParam !== "" && !isInspectorTabName(inspectorTabParam)) {
      setParams({ [PARAM_INSPECTOR]: "details" }, { replace: true });
    }
  }, [inspectorTabParam, setParams]);

  // The attach already taken for the selected session's *current live epoch*. Written by the
  // activation effect (selection), the liveness effect (a selected session that comes alive) and
  // `handleResume`, so no two of them can fire a `ConnectSession` for the same session.
  //
  // "Live epoch" is the load-bearing part: the claim is dropped once a *later* list snapshot reports
  // the session dormant. A session can die and be resumed repeatedly without the selection ever
  // changing, and each revival owes a fresh attach — a claim that only reset on selection change
  // would strand every resume after the first on an empty pane.
  const attachClaimRef = useRef<AttachClaim | null>(null);
  // Bumped once per session-list snapshot, so an attach claim can record which snapshot it was taken
  // under. Without it a resume could not be told apart from a stale dormant reading: `ResumeSession`
  // returns before the daemon's next `ListSessions`, so the list keeps reporting the session dormant
  // for up to one poll after the attach is already established.
  const listGenerationRef = useRef(0);
  // The session id the activation effect below has already run for. Keyed on the id (not a one-shot
  // boolean) so an inbound URL change — Back, a pasted link — activates the newly named session,
  // while a re-render for the same id does not re-connect it.
  const activatedSessionIdRef = useRef<string | null>(null);
  // Whether the activation effect has run at all. The first activation honours an `?inspector=` the
  // URL already carried (a deep link asked for that tab); later ones recompute the default for the
  // newly selected session, as selecting a session in the drawer has always done.
  const firstActivationPendingRef = useRef(true);
  // A deep link (`#/sessions/:id`) that resolves to no known session after the list loads shows a
  // not-found state instead of silently no-opping. Driven by local state so it dismisses on Home
  // without depending on a hash change.
  const [unknownSession, setUnknownSession] = useState(false);
  // Sessions ticked for bulk delete — a Set preserves insertion (selection) order, which the bulk
  // delete replays so sessions are removed in the order they were selected.
  const [selectedForDelete, setSelectedForDelete] = useState<Set<string>>(() => new Set());
  // Bulk-selection mode: off by default so the drawer reads as a plain list. The bottom minibar
  // toggles it on, which is what reveals the per-row checkboxes and the Select-all / Delete actions.
  const [selectionMode, setSelectionMode] = useState(false);

  const toggleSelectForDelete = useCallback((sessionId: string) => {
    setSelectedForDelete((prev) => {
      const next = new Set(prev);
      if (next.has(sessionId)) next.delete(sessionId);
      else next.add(sessionId);
      return next;
    });
  }, []);

  const enterSelectionMode = useCallback(() => setSelectionMode(true), []);

  // Leaving selection mode always clears the tick set — a stale selection must not survive into the
  // next time the operator opens the bar.
  const exitSelectionMode = useCallback(() => {
    setSelectionMode(false);
    setSelectedForDelete(new Set());
  }, []);

  // The project registry (daemon-level RPC over the selected-daemon common-room connection). Used
  // to resolve an unscoped session's project from its `repoPath` before the worktree RPCs — those
  // require a non-empty `project_id`. Falls back to an empty list when no daemon is selected yet or
  // the call fails, so the drawer still renders.
  const [projects, setProjects] = useState<ProjectEntry[]>([]);
  useEffect(() => {
    if (!client) {
      setProjects([]);
      return;
    }
    client
      .listProjects({ sessionToken })
      .then((res) => setProjects(res.projects))
      .catch(() => setProjects([]));
  }, [client, sessionToken]);

  // Default closed on mobile (the open 280px panel would cover the main pane);
  // open on desktop.
  const [sessionListOpen, setSessionListOpen] = useState(() => !detectIsMobile());
  const isMobile = useIsMobile();

  const { state: attachment, connectSession, resumeSession, deleteSession, signalSession, restore: restoreAttachment, reset: resetAttachment } = useSessionAttachment();

  const isConnected =
    attachment.status === "connected-livekit" || attachment.status === "connected-grpc";

  const connectedSessionId =
    attachment.status === "connected-grpc" || attachment.status === "connected-livekit"
      ? attachment.sessionId
      : null;

  // Per-session runtime registry: one entry per attached session, surviving focus switches.
  // The inspector reads byte counters + last-received from it for active sessions; inactive sessions
  // fall back to the daemon-sourced `SessionEntry` fields (req 5 dual source). Created once and kept
  // in a ref so the `SessionRuntimeRegistry` instance (and its cached `runtimes` snapshot) is stable
  // across renders — must be instantiated before any callback that touches it (buildSessionClient,
  // onSessionRoom, onSessionDisconnect).
  const runtimeRegistryRef = useRef<SessionRuntimeRegistry | null>(null);
  runtimeRegistryRef.current ??= new SessionRuntimeRegistry();
  const runtimeRegistry = runtimeRegistryRef.current;
  const runtimes = useSyncExternalStore(
    (listener) => runtimeRegistry.subscribe(listener),
    () => runtimeRegistry.runtimes,
    () => runtimeRegistry.runtimes,
  );

  // Session-scoped ConnectionService client (targets the coder participant
  // `daemon-{ownerInstanceId}-{sessionId}` = `attachment.livekitServerIdentity`). Built LAZILY — only
  // when the user actually invokes a session-scoped RPC (ExecuteTool, ClaimTerminalControl) — so that
  // lifecycle RPCs (Delete/Signal/Resume/Connect) and the auto-claim-on-attach stay daemon-direct and
  // do not record the session-participant identity. In production the session's own LiveKit `Room`
  // (captured via the terminal's `onRoom`) is the transport room; in tests the test-double
  // `liveKitFactory` ignores its `room` argument, so the common room is an acceptable stand-in.
  // Resolved through `sessionClientCache` so an unchanged route yields one stable client identity:
  // this callback is invoked inline while rendering, and consumers key stream effects on the client.
  const liveKitFactoryIsOverridden = useLiveKitTransportFactoryIsOverridden();
  const sessionClientCache = useSessionClientCache();
  const buildSessionClient = useCallback((): Client<typeof ConnectionService> | null => {
    if (!connectedSessionId) return null;
    if (attachment.status !== "connected-livekit") return null;
    const targetIdentity = attachment.livekitServerIdentity;
    if (!targetIdentity) return null;
    const sessionRoom =
      runtimeRegistry.get(connectedSessionId)?.room ?? (liveKitFactoryIsOverridden ? room : null);
    if (!sessionRoom) return null;
    return sessionClientCache.clientFor(targetIdentity, sessionRoom, () =>
      createClient(ConnectionService, liveKitFactory(sessionRoom, targetIdentity)),
    );
  }, [
    connectedSessionId,
    attachment,
    room,
    liveKitFactory,
    liveKitFactoryIsOverridden,
    runtimeRegistry,
    sessionClientCache,
  ]);

  // Capture a session's connected LiveKit `Room` (fired by the terminal after `room.connect`) so
  // `buildSessionClient` can route session-scoped RPCs over the session's own room in production.
  const onSessionRoom = useCallback(
    (sessionId: string, sessionRoom: Room) => {
      runtimeRegistry.setRoom(sessionId, sessionRoom);
    },
    [runtimeRegistry],
  );

  // Register a session's Agent-terminal text-insert (fired once its terminal mounts), so the
  // inspector's Files tab can insert an uploaded file's host path into the focused session's terminal
  // via a click/tap.
  const onSessionRegisterInsert = useCallback(
    (sessionId: string, insertInput: (text: string) => void) => {
      runtimeRegistry.setInsertInput(sessionId, insertInput);
    },
    [runtimeRegistry],
  );

  // Fold a session's terminal I/O bytes into its runtime counters as the terminal fires them (per
  // output chunk / input yield). The registry's `notify()` re-renders the screen (via
  // `useSyncExternalStore`), so the inspector's byte meter ticks live — even for a backgrounded session.
  const onSessionBytes = useCallback(
    (sessionId: string, delta: ByteDelta) => {
      makeByteTap(runtimeRegistry, sessionId)(delta);
    },
    [runtimeRegistry],
  );

  // The merged session list, refresh, and change events all live in one place: `SessionManager`. It
  // unions the selected host's sessions (from ListSessions, refreshed via the window-bound
  // `sessionsRefreshBridge`) with the live cross-host sessions observed as common-room coder
  // participants — LiveKit presence is the keep-alive that makes a non-selected host's session visible.
  const participants = useRoomParticipants(room);
  const { sessions: sortedSessions, addOptimisticSession, sessionMetadataBySessionId } = useSessionManager(
    client,
    sessionToken,
    participants,
    selectedInstanceId ?? "",
  );

  const selectedSession = useMemo(
    () => sortedSessions.find((s) => s.sessionId === selectedSessionId) ?? null,
    [sortedSessions, selectedSessionId],
  );

  // Disconnect a runtime terminal. Evicts the session's runtime; if it is the focused/attached
  // session, also resets the attachment so the screen re-evaluates state for the next selection.
  //
  // Whether the claim goes with the runtime is `claimAfterFeedEnd`'s to decide, off the same session
  // list the liveness effect below reads. For a terminal session the feed *is* the attach: a feed can
  // drop under a session the daemon still reports alive (a `pty_done` on a live agent), and holding a
  // claim for an attach that no longer exists is what would strand the pane on the reconnect
  // placeholder — that effect returns early on the claim, and a live session offers no Resume button
  // to recover by hand. For a workflow-owned session the feed is the hidden runtime layer's, which
  // has no PTY to stream and ends at once; releasing on it would have that effect re-attach on every
  // list snapshot for as long as the session lives.
  const onSessionDisconnect = useCallback(
    (sessionId: string) => {
      runtimeRegistry.disconnect(sessionId);
      const session = sortedSessions.find((s) => s.sessionId === sessionId);
      if (session) {
        attachClaimRef.current = claimAfterFeedEnd({ claim: attachClaimRef.current, session });
      } else if (attachClaimRef.current?.sessionId === sessionId) {
        // The list no longer carries the session, so there is no pane to hold the attach for.
        attachClaimRef.current = null;
      }
      if (sessionId === connectedSessionId) resetAttachment();
    },
    [runtimeRegistry, sortedSessions, connectedSessionId, resetAttachment],
  );

  // Count each session-list snapshot as it arrives. Done during render (not in an effect) so a claim
  // taken from an event handler in the same commit records the snapshot the operator was actually
  // looking at, rather than one the effect queue has not stamped yet.
  const listGeneration = useMemo(() => {
    listGenerationRef.current += 1;
    return listGenerationRef.current;
  }, [sortedSessions]);

  // A pr-stack orchestrator is itself idle while its children do the work; when a child is live,
  // the orchestrator must stay reachable in the drawer (grouped with its children) rather than
  // collapsing into the disconnected "Remaining" partition mid-flight. Reflect that by treating an
  // orchestrator as active whenever a session it owns is active. This only affects drawer grouping;
  // the main pane keeps the raw list so branch→session resolution reads each child's true activity.
  const drawerSessions = useMemo(() => {
    const activeOrchestratorIds = new Set(
      sortedSessions
        .filter((s) => s.isActive && s.orchestratorSessionId.length > 0)
        .map((s) => s.orchestratorSessionId),
    );
    if (activeOrchestratorIds.size === 0) return sortedSessions;
    return sortedSessions.map((s) =>
      !s.isActive && activeOrchestratorIds.has(s.sessionId) ? { ...s, isActive: true } : s,
    );
  }, [sortedSessions]);

  // Register a session in the runtime registry on successful attach, storing the connection
  // params so the runtime layer can render its terminal independently of the focused attachment.
  // `lastDataReceivedAt` starts at the attach moment so the inspector reads "0s ago" before the
  // first DataReceived event lands. Re-attach (an existing backgrounded runtime) refreshes the
  // connection params without resetting byte counters.
  useEffect(() => {
    if (attachment.status !== "connected-livekit" && attachment.status !== "connected-grpc") return;
    if (!attachment.sessionId) return;
    const conn: SessionRuntimeConnection =
      attachment.status === "connected-livekit"
        ? {
            status: "connected-livekit",
            livekitUrl: attachment.livekitUrl,
            livekitRoom: attachment.livekitRoom,
            livekitServerIdentity: attachment.livekitServerIdentity,
            identity: attachment.identity,
          }
        : { status: "connected-grpc", livekitUrl: "", livekitRoom: "", livekitServerIdentity: "", identity: "" };
    const existing = runtimeRegistry.get(attachment.sessionId);
    if (!existing) {
      runtimeRegistry.add(attachment.sessionId, {
        sessionId: attachment.sessionId,
        attached: true,
        ...conn,
        bytesIn: 0,
        bytesOut: 0,
        lastDataReceivedAt: Date.now(),
      });
    } else {
      runtimeRegistry.updateConnection(attachment.sessionId, conn);
    }
    runtimeRegistry.focus(attachment.sessionId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [attachment.status, attachment.sessionId]);

  // Compute the inspector traffic source for the selected session: live runtime (active) wins;
  // otherwise the daemon-sourced `SessionEntry` fields (inactive / non-LiveKit sessions).
  const selectedTraffic = useMemo(() => {
    if (!selectedSession) return null;
    const live = runtimeRegistry.get(selectedSession.sessionId);
    if (live) {
      return { bytesIn: live.bytesIn, bytesOut: live.bytesOut, lastDataReceivedAt: live.lastDataReceivedAt };
    }
    const fromEntry = Number(selectedSession.bytesIn ?? 0n) || 0;
    const fromEntryOut = Number(selectedSession.bytesOut ?? 0n) || 0;
    const lastStr = selectedSession.lastDataReceivedAt ?? "";
    const lastNum = lastStr ? Number(lastStr) : null;
    return {
      bytesIn: fromEntry,
      bytesOut: fromEntryOut,
      lastDataReceivedAt: Number.isFinite(lastNum) ? (lastNum as number) : null,
    };
  }, [selectedSession, runtimes, runtimeRegistry]);

  // The daemon that owns the selected session — cross-host interaction (connect, resume, delete,
  // terminate) must reach that daemon, not the selected one.
  const selectedOwningHost = useMemo(
    () => (selectedSession ? owningHostForSession(selectedSession, selectedInstanceId ?? "") : null),
    [selectedSession, selectedInstanceId],
  );
  const activeClient = useDaemonClientFor(ConnectionService, selectedOwningHost);

  // Human-readable host label for a daemon instance id, with the local daemon's " (this daemon)"
  // suffix stripped — used for the owning-host badge on cross-host rows.
  const hostLabelForInstance = useCallback(
    (instanceId: string): string => {
      const host = daemons.find((d) => d.instanceId === instanceId);
      return (host?.label ?? instanceId).replace(/ \(this daemon\)$/, "");
    },
    [daemons],
  );

  // Key-press shortcuts for the connected session's tool (shown as the mobile overlay).
  const mobileShortcuts = useMemo(
    () =>
      resolveShortcutsForSession(
        isCliTerminalSession(selectedSession?.agent ?? ""),
        selectedSession?.tool ?? "",
      ),
    [selectedSession],
  );

  /**
   * Activate whichever session the URL names, once the list has loaded: set the inspector's default
   * state and attach the session. One effect serves every way the selection can change — a drawer
   * click, a deep link on load, Back/Forward, a pasted link — because they all just change the URL.
   *
   * Attaching here rather than in the click handler is what makes Back re-attach: `handleSelect`
   * only navigates.
   */
  useEffect(() => {
    if (!selectedSessionId) {
      activatedSessionIdRef.current = null;
      setUnknownSession(false);
      // `inspector` / `full` / `code` describe a session's panes; with no session selected they
      // describe nothing. Drop them so "back to sessions" cannot leave a URL claiming an open
      // inspector over an empty pane. `replace` — this is cleanup, not a destination.
      if (location.params[PARAM_INSPECTOR] || location.params[PARAM_FULL] || location.params[PARAM_CODE]) {
        setParams(
          { [PARAM_INSPECTOR]: null, [PARAM_FULL]: null, [PARAM_CODE]: null },
          { replace: true },
        );
      }
      return;
    }
    if (activatedSessionIdRef.current === selectedSessionId) return;
    if (sortedSessions.length === 0) return; // list not loaded yet — retry on the next change
    const session = sortedSessions.find((s) => s.sessionId === selectedSessionId);
    activatedSessionIdRef.current = selectedSessionId;
    const honourUrlInspector = firstActivationPendingRef.current && inspectorTabParam !== "";
    firstActivationPendingRef.current = false;

    if (!session) {
      // The URL names an id that is not in the loaded list — surface a not-found state.
      setUnknownSession(true);
      return;
    }
    setUnknownSession(false);

    // A deep link that asked for a tab keeps it; otherwise the inspector takes its default state,
    // which is closed whatever the session's liveness: an active session shows its terminal, an
    // inactive one shows its recorded activities, and neither has the drawer opened for it.
    if (!honourUrlInspector) {
      const selected = nextInspectorState({ open: false, expanded: false }, { type: "select" });
      applyInspectorState(selected.open ? "open" : "closed", { replace: true });
    }

    // Fast path — the session's runtime is already mounted in the registry (it was attached
    // earlier and stays alive across focus switches). Restore the attachment from the registry's
    // stored connection params so the screen re-evaluates state for the newly selected session
    // WITHOUT an RPC round-trip: no re-connect, no fresh ClaimTerminalControl, no token race, and
    // the existing terminal stream keeps flowing. The registry effect below re-focuses it.
    // The claim is taken inside each branch rather than for any registry hit: a registry entry that
    // is no longer connected restores nothing, so claiming on it would block the resume this dormant
    // session is about to be given.
    const existing = runtimeRegistry.get(selectedSessionId);
    if (existing?.status === "connected-livekit") {
      attachClaimRef.current = { sessionId: selectedSessionId, listGeneration };
      restoreAttachment({
        status: "connected-livekit",
        sessionId: selectedSessionId,
        livekitUrl: existing.livekitUrl ?? "",
        livekitRoom: existing.livekitRoom ?? "",
        livekitServerIdentity: existing.livekitServerIdentity ?? "",
        identity: existing.identity ?? "",
      } satisfies SessionAttachmentState);
      return;
    }
    if (existing?.status === "connected-grpc") {
      attachClaimRef.current = { sessionId: selectedSessionId, listGeneration };
      restoreAttachment({
        status: "connected-grpc",
        sessionId: selectedSessionId,
      } satisfies SessionAttachmentState);
      return;
    }

    // Slow path — not yet attached. Reset so the attachment effect re-evaluates state for the new
    // selection, then connect to this session's owning daemon. `activeClient` is derived from
    // `selectedSession`, which this render may not have caught up to, so build the client for this
    // session's owner directly rather than reading `activeClient` here.
    resetAttachment();
    // An attach is owed for a live session only; a dormant one has no process to attach to and shows
    // its recorded activities instead. Recording which id this effect took the attach for is what
    // keeps the liveness effect below from issuing a second `ConnectSession` for the same session.
    attachClaimRef.current = session.isActive
      ? { sessionId: selectedSessionId, listGeneration }
      : null;
    if (session.isActive) {
      const owningClient = clientForHost(owningHostForSession(session, selectedInstanceId ?? ""));
      if (owningClient) {
        connectSession(selectedSessionId, sessionToken, owningClient).catch((err) => {
          console.debug("[SessionsDrawerScreen] connectSession error", err);
        });
      }
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedSessionId, sortedSessions]);

  /**
   * Track the selected session's liveness across polls: attach it when it comes alive, and release
   * the attach claim when it dies.
   *
   * A session can come alive under the operator — resumed from the pane's top bar, or resumed
   * elsewhere and observed on the next list poll — and the activation effect above only runs once per
   * *selection*, so without this the pane would keep showing recorded activities until the session
   * was re-selected. Attaching here is what lets the base view return to the terminal on its own, as
   * `docs/ft/web/inactive-session-activities.md` § Resume states.
   *
   * Releasing on death matters just as much: the same session can be resumed again without the
   * selection ever changing, and that resume owes a fresh `ConnectSession`. Holding the claim past
   * the death would silently swallow every resume after the first.
   *
   * Which of the three the snapshot owes is `attachActionForSnapshot`'s to decide — including why a
   * dormant reading on the claim's own snapshot is not yet evidence of death.
   */
  useEffect(() => {
    if (!selectedSession) return;
    const action = attachActionForSnapshot({
      session: selectedSession,
      claim: attachClaimRef.current,
      listGeneration,
    });
    switch (action) {
      case "hold":
        return;
      case "release":
        attachClaimRef.current = null;
        return;
      case "attach":
        break;
    }
    const owningClient = clientForHost(
      owningHostForSession(selectedSession, selectedInstanceId ?? ""),
    );
    if (!owningClient) return;
    attachClaimRef.current = { sessionId: selectedSession.sessionId, listGeneration };
    connectSession(selectedSession.sessionId, sessionToken, owningClient).catch((err) => {
      console.debug("[SessionsDrawerScreen] connectSession error", err);
    });
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedSession, selectedInstanceId, clientForHost, listGeneration]);

  // React to attachment status changes: an attach **error** opens the inspector so the operator sees
  // the problem — that is a failure to surface, not a liveness state. Every other status leaves the
  // drawer alone. A session that simply has nothing to attach to ("idle" for an inactive session) no
  // longer opens it: that session's pane shows its recorded activities instead.
  useEffect(() => {
    if (!selectedSessionId) return;
    if (attachment.status === "error" && inspectorState !== "expanded") {
      applyInspectorState("open", { replace: true });
    }
  // These transitions are driven by the attachment, not by the URL: re-running them whenever
  // `inspectorState`/`applyInspectorState` change would fight the operator's own toggles.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [attachment.status, selectedSessionId]);

  // Selecting a session is a navigation plus one acknowledgement — the activation effect above does
  // the inspector and attachment work, so Back and a pasted link get the same treatment for
  // everything that decides what the screen shows.
  const handleSelectSession = (sessionId: string) => {
    // The exception is the row's own indicator: clicking a row is the operator *deciding* to look at
    // what that session had to say, so its outstanding notifications settle here (FR6). It sits on
    // the click rather than in the activation effect deliberately — a reload or a Back onto a
    // session the operator is not reading should not silently clear a dot they never saw. Only the
    // notification-driven half clears: a `pending_elicitation` is an unanswered gate and keeps its
    // yellow until it is actually answered (see `sessionIndicatorFor`).
    sessionNotificationRegistry.markSeen(sessionId, Date.now());
    // On mobile the list is a full-screen overlay — close it so the terminal is visible.
    if (isMobile) setSessionListOpen(false);
    navigate(sessionsDrawerPathForSession(sessionId));
  };

  const handleResume = (sessionId: string) => {
    if (!activeClient) return;
    // `ResumeSession` returns the session's LiveKit coordinates and `useSessionAttachment` puts the
    // screen straight into `connected-livekit` — the attach for this live epoch is already done.
    // Claiming it here stops the liveness effect from firing a second `ConnectSession` when the next
    // list poll reports the session alive, which would mint a fresh browser identity and bounce the
    // terminal through a reconnect and another `ClaimTerminalControl`. The claim is stamped with the
    // snapshot the operator acted on, so the dormant readings still in flight cannot revoke it.
    attachClaimRef.current = { sessionId, listGeneration };
    resumeSession(sessionId, sessionToken, activeClient).catch((err) => {
      // The resume never landed, so no attach was taken after all — release the claim so the liveness
      // effect can still attach if the session turns out to be alive.
      attachClaimRef.current = null;
      console.debug("[SessionsDrawerScreen] resumeSession error", err);
    });
  };

  const handleDelete = (sessionId: string) => {
    if (!activeClient) return;
    deleteSession(sessionId, sessionToken, activeClient).catch((err) => {
      console.debug("[SessionsDrawerScreen] deleteSession error", err);
    });
  };

  // Delete every selected session in selection (insertion) order, routing each delete to the
  // session's owning daemon. Sequential so the daemon processes them in the same order the operator
  // ticked the rows, and so a single failure doesn't abandon the remaining deletes silently.
  // Select-all toggles against the full visible list: if every session is already ticked, clear;
  // otherwise tick them all (fresh Set so insertion order matches the current list order).
  const toggleSelectAll = useCallback(() => {
    setSelectedForDelete((prev) => {
      const allIds = sortedSessions.map((s) => s.sessionId);
      const allSelected = allIds.length > 0 && allIds.every((id) => prev.has(id));
      return allSelected ? new Set() : new Set(allIds);
    });
  }, [sortedSessions]);

  const handleBulkDelete = useCallback(async () => {
    const ids = [...selectedForDelete];
    for (const id of ids) {
      const session = sortedSessions.find((s) => s.sessionId === id);
      const owner = session
        ? owningHostForSession(session, selectedInstanceId ?? "")
        : (selectedInstanceId ?? "");
      const targetClient = clientForHost(owner) ?? client;
      if (!targetClient) continue;
      try {
        await deleteSession(id, sessionToken, targetClient);
      } catch (err) {
        console.debug("[SessionsDrawerScreen] bulk deleteSession error", err);
      }
    }
    setSelectedForDelete(new Set());
    setSelectionMode(false);
    requestSessionsRefresh();
  }, [selectedForDelete, sortedSessions, selectedInstanceId, clientForHost, client, deleteSession, sessionToken]);

  const handleUnknownHome = () => {
    setUnknownSession(false);
    navigate(SESSIONS_DRAWER_ROUTE);
  };

  // A pr-stack orchestrator's "Start session" CTA spawns a child immediately in the
  // background (no navigation, no auto-connect — the operator stays on the orchestrator's
  // chat screen). Add a minimal optimistic entry so the drawer reflects it right away. Unlike
  // handleTerminate's refresh() (which needs an authoritative isActive from the daemon), a full
  // refetch here isn't safe to chain: the daemon's session-list enrichment may not have indexed the
  // brand-new session yet. The optimistic overlay is merged into the list (a fetched entry with the
  // same id always wins), and its remaining fields fill in on the next fan-out refresh.
  const handleChildSessionStarted = (entry: {
    sessionId: string;
    recipe: string;
    orchestratorSessionId: string;
    projectId: string;
  }) => {
    addOptimisticSession(
      create(SessionEntrySchema, {
        sessionId: entry.sessionId,
        recipe: entry.recipe,
        orchestratorSessionId: entry.orchestratorSessionId,
        projectId: entry.projectId,
        isActive: true,
        createdAt: new Date().toISOString(),
      }),
    );
  };

  const handleTerminate = (sessionId: string) => {
    if (!activeClient) return;
    signalSession(sessionId, Signal.SIGTERM, sessionToken, activeClient)
      .catch((err) => {
        // Common cause: the session already ended (e.g. process exited on its own) before this
        // click reached the daemon — refresh() below still corrects the stale `isActive` that
        // caused the "Terminate" button to be shown for an already-dead session.
        console.debug("[SessionsDrawerScreen] signalSession error", err);
      })
      .finally(() => {
        // The daemon computes `isActive` from live PID liveness, not from a push update — refetch
        // so the row (and its "Terminate" button) reflects the session's actual current state.
        requestSessionsRefresh();
      });
  };

  const handleInspectorToggle = () => {
    const prevState = { open: inspectorState !== "closed", expanded: inspectorState === "expanded" };
    const next = nextInspectorState(prevState, { type: "toggle" });
    applyInspectorState(next.open ? (next.expanded ? "expanded" : "open") : "closed");
  };

  const handleInspectorClose = () => applyInspectorState("closed");

  // Insert an uploaded file's host path (Files tab → Insert / tap) into the focused session's
  // terminal, shell-escaped exactly as a native terminal file-drag would type it. The Files tab
  // closes the inspector itself (via `onCloseInspector`), so this only performs the insert.
  const handleInsertPathIntoTerminal = (hostPath: string) => {
    const focusedId = runtimeRegistry.focusedSessionId;
    if (!focusedId) return;
    runtimeRegistry.get(focusedId)?.insertInput?.(joinQuotedPaths([hostPath]));
  };
  const handleInspectorExpand = () => applyInspectorState("expanded");
  const handleInspectorRestore = () => applyInspectorState("open");

  const handleCreateSession = () => navigate(SESSIONS_NEW_ROUTE);
  const handleCancelCreate = () => navigate(SESSIONS_DRAWER_ROUTE);
  const handleSessionCreated = (sessionId: string) => {
    // Auto-close the sessions drawer so the new session's terminal is unobstructed.
    setSessionListOpen(false);
    navigate(sessionsDrawerPathForSession(sessionId));
    if (!client) return;
    connectSession(sessionId, sessionToken, client).catch((err) => {
      console.debug("[SessionsDrawerScreen] connectSession after create error", err);
    });
    // Refresh the sessions list so the newly-created session appears in the drawer
    // and selectedSession resolves to a non-null value.
    requestSessionsRefresh();
  };

  return (
    <UploadProgressProvider>
    <TooltipProvider delayDuration={0}>
      {/* 100dvh (via AppShell fullbleed): on mobile 100vh includes the area behind the browser
          chrome, which would push the bottom keyboard bar off the visible screen. */}
      <AppShell
        variant="fullbleed"
        title="Sessions"
        onNavigate={onNavigate}
        dataTestId="sessions-drawer-screen"
      >
        <div className="flex flex-1 min-h-0 overflow-hidden relative">
          {isMobile && !sessionListOpen && (
            <button
              type="button"
              data-testid="sessions-drawer-open-overlay-btn"
              onClick={() => setSessionListOpen(true)}
              title="Open session list"
              className="absolute top-2 left-2 z-20 flex items-center justify-center h-9 w-9 rounded-md border border-border bg-background/90 text-foreground shadow-md backdrop-blur-sm hover:bg-muted transition-colors"
            >
              <PanelLeftOpen className="h-5 w-5" />
            </button>
          )}
          <SessionDrawer
            sessions={drawerSessions}
            selectedSessionId={selectedSessionId}
            onSelectSession={handleSelectSession}
            onCreateSession={handleCreateSession}
            isOpen={sessionListOpen}
            onClose={() => setSessionListOpen(false)}
            onOpen={() => setSessionListOpen(true)}
            isMobile={isMobile}
            selectedInstanceId={selectedInstanceId ?? ""}
            hostLabelForInstance={hostLabelForInstance}
            sessionMetadataBySessionId={sessionMetadataBySessionId}
            selectedForDelete={selectedForDelete}
            selectionMode={selectionMode}
            onToggleSelect={selectionMode ? toggleSelectForDelete : undefined}
            onEnterSelectionMode={enterSelectionMode}
            onExitSelectionMode={exitSelectionMode}
            onSelectAll={toggleSelectAll}
            onBulkDelete={() => {
              void handleBulkDelete();
            }}
          />
          {/* A bad deep link surfaces "not found" in the detail pane only — the session list
              stays visible so the operator can pick a valid session. */}
          {unknownSession ? (
            <div className="flex flex-1 min-h-0 items-center justify-center p-6">
              <div
                data-testid="terminal-route-unknown-session"
                className="rounded-md border border-destructive/40 bg-destructive/5 p-4"
              >
                <p className="mb-3 text-sm text-foreground">
                  Session not found or no longer available.
                </p>
                <Button
                  type="button"
                  variant="secondary"
                  data-testid="terminal-route-unknown-session-home"
                  onClick={handleUnknownHome}
                >
                  Back to sessions
                </Button>
              </div>
            </div>
          ) : (
            <SessionMainPane
              selectedSession={selectedSession}
              attachment={attachment}
              inspectorState={inspectorState}
              onToggleInspector={handleInspectorToggle}
              onInspectorClose={handleInspectorClose}
              onInspectorExpand={handleInspectorExpand}
              onInspectorRestore={handleInspectorRestore}
              onResume={handleResume}
              onDelete={handleDelete}
              onTerminate={handleTerminate}
              isCreating={mode === "creating"}
              client={mode === "creating" ? (client ?? undefined) : (activeClient ?? client ?? undefined)}
              tokenClient={tokenClient}
              sessionToken={sessionToken}
              onCancelCreate={handleCancelCreate}
              onSessionCreated={handleSessionCreated}
              room={room}
              mobileShortcuts={mobileShortcuts}
              onChildSessionStarted={handleChildSessionStarted}
              onSwitchPeer={handleSelectSession}
              traffic={selectedTraffic}
              projects={projects}
              runtimes={runtimes}
              sessions={sortedSessions}
              focusedRuntimeId={runtimeRegistry.focusedSessionId}
              onSessionRoom={onSessionRoom}
              onSessionRegisterInsert={onSessionRegisterInsert}
              onInsertPathIntoTerminal={handleInsertPathIntoTerminal}
              onSessionDisconnect={onSessionDisconnect}
              onSessionBytes={onSessionBytes}
              buildSessionClient={buildSessionClient}
              liveKitFactory={liveKitFactory}
              liveKitFactoryIsOverridden={liveKitFactoryIsOverridden}
            />
          )}
        </div>
        <HostStatsFooter
          attachment={attachment}
          runtimes={runtimes}
          runtimeRegistry={runtimeRegistry}
        />
      </AppShell>
    </TooltipProvider>
    </UploadProgressProvider>
  );
}
