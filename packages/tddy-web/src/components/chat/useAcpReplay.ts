import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from "react";
import { fromBinary } from "@bufbuild/protobuf";
import type { Client } from "@connectrpc/connect";
import { type ConnectionService, StreamMode } from "../../gen/connection_pb";
import { AcpAgentMessageSchema } from "../../gen/tddy/acp/v1/acp_pb";
import { agentActivityRegistry } from "../sessions/agentActivityRegistry";
import { createReplayProjector, projectReplayFrames } from "./acpReplayProjection";
import type { UseAgentChatResult } from "./useAgentChat";

/** Frames per transcript page — the tail the feed opens on, and the size of every page paged in
 *  behind it. Mirrors `tddy_service::acp_replay::DEFAULT_REPLAY_PAGE_SIZE`; stated explicitly rather
 *  than left at 0 so the request says what it wants instead of inheriting whatever the host defaults
 *  to. */
export const REPLAY_PAGE_SIZE = 100;

/** The read-only transcript surface the Agent Activity overlay renders. Extends the shared
 *  {@link UseAgentChatResult} (so `AgentChatView` can render it interchangeably) with the overlay's
 *  icon/badge signals and the lazy-snapshot controls. Send/answer methods are inert — a replay is
 *  not interactive. */
export interface UseAcpReplayResult extends UseAgentChatResult {
  /** True once the count feed reports at least one persisted activity frame. */
  hasActivity: boolean;
  /** True once the count feed has answered at all, whatever it said. Distinguishes "this session
   *  recorded nothing" from "the count has not arrived yet" — a surface that renders that difference
   *  must wait for this before claiming the former. */
  countLoaded: boolean;
  /** Activity frames counted since the overlay was last opened (drives the unread badge). */
  unreadCount: number;
  /** Mark the current count as seen (clears the unread badge). */
  markSeen: () => void;
  /** Open the heavy transcript snapshot for this session — call on first overlay open. Reuses the
   *  cached transcript on a later open (or a switch-back), so the snapshot stream opens at most once
   *  per delivered transcript. */
  loadSnapshot: () => void;
  /** True once the snapshot pull has delivered this session's transcript. */
  snapshotLoaded: boolean;
  /** True once the loaded range reaches the transcript head — nothing older is left to page in. */
  atOldest: boolean;
  /** True while a page of older history is in flight. */
  loadingOlder: boolean;
  /** Page in the history immediately before the loaded range and prepend it. A no-op while
   *  {@link atOldest}, while a fetch is already in flight, or before the transcript feed has
   *  delivered a cursor to page back from. */
  loadOlder: () => void;
}

const NOOP_SEND = () => false;

/**
 * Subscribes to `ConnectionService.StreamAcpReplay` for one session in two phases, backed by the
 * module-level {@link agentActivityRegistry} so state survives a session switch:
 *
 * - a **count** feed (`COUNT_THEN_LIVE`), opened while the session is focused, whose frames carry
 *   `activity_count` (no transcript payload). This drives `hasActivity` and `unreadCount` cheaply,
 *   without pulling the full transcript.
 * - a **transcript** feed (`TAIL_THEN_LIVE`, {@link REPLAY_PAGE_SIZE} frames), opened lazily by
 *   {@link UseAcpReplayResult.loadSnapshot} — on the first overlay open, or straight away for the
 *   Activities view, which IS the transcript. It replays only the **newest** page of the recorded
 *   transcript, then tails live, so a session with a multi-megabyte history costs one page to show
 *   the end of it. Its ACP `session_update` frames are projected into the read-only chat transcript
 *   and cached in the registry, so a switch away and back reuses it rather than re-pulling. A pull
 *   only counts as done once its frames actually land, so one that is cancelled mid-flight (a
 *   remount, a session switch) is retried instead of caching an empty transcript.
 *
 * Older history is reached by paging **backwards**: the feed's first frame carries the absolute
 * position its page starts at, and {@link UseAcpReplayResult.loadOlder} fetches the page before that
 * cursor through the unary `GetAcpReplayPage`, prepending it above the loaded range. A failed fetch
 * changes nothing except clearing the in-flight flag, so a later scroll retries it.
 *
 * Frame projection lives in {@link createReplayProjector} and mirrors the live ACP path — see there
 * for how each `session_update` variant becomes an entry. Each entry's timestamp is the frame's
 * `SessionNotification.timestamp_unix_ms`, so elapsed badges reflect the recorded timeline.
 */
export function useAcpReplay(args: {
  sessionId: string;
  sessionToken: string;
  client: Client<typeof ConnectionService>;
}): UseAcpReplayResult {
  const { sessionId, sessionToken, client } = args;

  const state = useSyncExternalStore(
    (listener) => agentActivityRegistry.subscribe(listener),
    () => agentActivityRegistry.get(sessionId),
  );

  const count = state?.count ?? 0;
  const countLoaded = state?.countLoaded ?? false;
  const seenCount = state?.seenCount ?? 0;
  const messages = state?.messages ?? [];
  const snapshotLoaded = state?.snapshotLoaded ?? false;
  const atOldest = state?.atOldest ?? false;
  const loadingOlder = state?.loadingOlder ?? false;

  // Both stream effects key on `client`, so a genuine routing change (daemon-direct → session-scoped
  // once the session's room connects) re-subscribes over the new transport. That is only safe because
  // the client's *identity* is stable at the source: hosts build it inline while rendering
  // (`buildSessionClient?.() ?? client` in `SessionMainPane`) but resolve the build through
  // `SessionClientCache`, so an unchanged route hands back the same client and a mere re-render
  // cannot tear a subscription down mid-flight.

  // The count feed runs while the session is focused: cheap frames carrying only `activity_count`.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        for await (const frame of client.streamAcpReplay({
          sessionToken,
          sessionId,
          daemonInstanceId: "",
          mode: StreamMode.COUNT_THEN_LIVE,
        })) {
          if (cancelled) break;
          agentActivityRegistry.setCount(sessionId, Number(frame.activityCount));
        }
      } catch (err) {
        // A stream aborted on unmount surfaces as an AbortError; ignore it (no fallback fabrication).
        if (!cancelled) console.debug("[useAcpReplay] count stream error", err);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [client, sessionId, sessionToken]);

  // Which session's transcript has been requested via `loadSnapshot`. Held per-hook (not per-session)
  // so switching away to an unvisited session does NOT eagerly open its transcript.
  const [snapshotSession, setSnapshotSession] = useState<string | null>(null);
  const loadSnapshot = useCallback(() => setSnapshotSession(sessionId), [sessionId]);

  // The transcript feed: opened lazily once `loadSnapshot` targets the current session, and only when
  // the registry has not already loaded it (a switch-back reuses the cached range and its cursor).
  useEffect(() => {
    if (snapshotSession !== sessionId) return;
    if (agentActivityRegistry.get(sessionId)?.snapshotLoaded) return;

    let cancelled = false;
    // Opened on the first frame, which is what states the page's absolute start — the projector needs
    // it to scope its entry keys, since a page paged in later shares one rendered list with this one.
    let projector: ReturnType<typeof createReplayProjector> | null = null;

    (async () => {
      try {
        for await (const frame of client.streamAcpReplay({
          sessionToken,
          sessionId,
          daemonInstanceId: "",
          mode: StreamMode.TAIL_THEN_LIVE,
          pageSize: REPLAY_PAGE_SIZE,
        })) {
          if (cancelled) break;
          // The pull counts as done from its first delivered frame onwards — marking it earlier (at
          // subscribe time) would leave a pull cancelled before any frame landed cached as an empty
          // transcript that no later open would refill.
          agentActivityRegistry.markSnapshotLoaded(sessionId);
          if (!projector) {
            // The first frame's absolute position IS the reverse cursor: 0 means the page already
            // reaches the transcript head, so there is nothing older to page in.
            const firstSeq = Number(frame.seq);
            projector = createReplayProjector(firstSeq);
            agentActivityRegistry.setOldestSeq(sessionId, firstSeq, firstSeq === 0);
          }
          const entries = projector.append(
            fromBinary(AcpAgentMessageSchema, frame.acpAgentMessage),
          );
          if (!cancelled) agentActivityRegistry.setMessages(sessionId, entries);
        }
      } catch (err) {
        // A stream aborted on unmount surfaces as an AbortError; ignore it. Any other error while
        // still mounted leaves the transcript showing what it has (no fallback fabrication).
        if (!cancelled) console.debug("[useAcpReplay] transcript stream error", err);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [client, sessionId, sessionToken, snapshotSession]);

  // Exactly one page fetch is in flight at a time. Held in a ref rather than read off the registry
  // because a second scroll can cross the threshold before a `loadingOlder` write has re-rendered,
  // and two requests for the same cursor would page the same frames in twice.
  const olderPageInFlight = useRef(false);

  const loadOlder = useCallback(() => {
    const loaded = agentActivityRegistry.get(sessionId);
    const cursor = loaded?.oldestSeq ?? null;
    // Nothing to page back from: the feed has not stated where the range starts, or the range
    // already reaches the transcript head.
    if (cursor === null || cursor <= 0 || loaded?.atOldest) return;
    if (olderPageInFlight.current) return;

    olderPageInFlight.current = true;
    agentActivityRegistry.setLoadingOlder(sessionId, true);
    (async () => {
      try {
        const page = await client.getAcpReplayPage({
          sessionToken,
          sessionId,
          daemonInstanceId: "",
          beforeSeq: BigInt(cursor),
          pageSize: REPLAY_PAGE_SIZE,
        });
        const firstSeq = Number(page.firstSeq);
        const entries = projectReplayFrames(
          page.frames.map((bytes) => fromBinary(AcpAgentMessageSchema, bytes)),
          firstSeq,
        );
        // Keyed by the `sessionId` this fetch was issued for, so a page that resolves after a session
        // switch lands under its own session rather than the one now on screen.
        agentActivityRegistry.prependMessages(sessionId, entries, firstSeq, page.atOldest);
      } catch (err) {
        // The loaded range is left exactly as it was: no fabricated page, no partial one, and the
        // range is NOT closed — clearing the in-flight flag below is what makes a later scroll retry.
        console.debug("[useAcpReplay] older page fetch failed", err);
      } finally {
        olderPageInFlight.current = false;
        agentActivityRegistry.setLoadingOlder(sessionId, false);
      }
    })();
  }, [client, sessionId, sessionToken]);

  const markSeen = useCallback(() => {
    agentActivityRegistry.markSeen(sessionId);
  }, [sessionId]);

  const unreadCount = Math.max(0, count - seenCount);

  return {
    messages,
    elicitations: [],
    sendPrompt: NOOP_SEND,
    pendingQuestion: null,
    answerSelect: NOOP_SEND,
    answerOther: NOOP_SEND,
    answerMultiSelect: NOOP_SEND,
    streamError: null,
    sendError: null,
    workflowError: null,
    hasActivity: count > 0,
    countLoaded,
    unreadCount,
    markSeen,
    loadSnapshot,
    snapshotLoaded,
    atOldest,
    loadingOlder,
    loadOlder,
  };
}
