import { useRef, useState, type MutableRefObject, type RefObject } from "react";
import { TerminalHistoryForwardLoader } from "tddy-client-web";
import { feedSupportsHistory, type TerminalFeed } from "tddy-client-web/connections/terminal";
import type { GhosttyTerminalHandle } from "../terminal/types";
import { dHistory, decModeChanges, traceModes, tuiOwnsNavigation } from "./sessionDiagnostics";

/**
 * The history double buffer: two terminals share one rect and `view` decides which is foreground.
 * Owns the forward fill of the older-history page terminal and which affordances are offered.
 */
export function useHistoryDoubleBuffer({
  feed,
  olderTermRef,
  olderReadyRef,
  olderBufferRef,
  currentOffsetRef,
  refreshMirrors,
}: {
  feed: TerminalFeed;
  olderTermRef: RefObject<GhosttyTerminalHandle | null>;
  olderReadyRef: MutableRefObject<boolean>;
  olderBufferRef: MutableRefObject<Uint8Array[]>;
  /** Cumulative output offset received so far; bounds the forward fill. */
  currentOffsetRef: MutableRefObject<bigint>;
  refreshMirrors: () => void;
}) {
  // Overlay double-buffer paging state. Two ghostty-web terminals share the same rect; `view`
  // decides which is foreground (visible, interactive). The live terminal (scrollback 0, always
  // pinned to the live tip) always stays mounted and keeps receiving the stream, so swapping back
  // to "live" is instant. `loading` shows the loading indicator while the background page terminal
  // is forward-filled; `filled` records that the page terminal has been populated (enables instant
  // re-swap to history).
  const loaderRef = useRef<TerminalHistoryForwardLoader | null>(null);
  const anchorRef = useRef<{ endOffset: bigint; atOldest: boolean } | null>(null);
  const [anchor, setAnchor] = useState<{ endOffset: bigint; atOldest: boolean } | null>(null);
  const fillingRef = useRef(false);
  const [view, setView] = useState<"live" | "page">("live");
  const [loading, setLoading] = useState(false);
  const [filled, setFilled] = useState(false);
  // Whether the live terminal's program owns navigation (see `tuiOwnsNavigation`) — re-read after
  // every chunk that switches a mode, since ghostty-web holds the state and does not report changes.
  const [liveTuiOwnsNavigation, setLiveTuiOwnsNavigation] = useState(false);
  // The terminal parses written data after `write` returns, so the modes a chunk switches are read
  // when the parse lands: a listener per terminal re-reads them after every parse.
  const modeWatchRef = useRef<{ term: GhosttyTerminalHandle; subscription: { dispose(): void } } | null>(null);
  const noteLiveModeSwitches = (term: GhosttyTerminalHandle, data: Uint8Array) => {
    const modes = decModeChanges(data);
    if (modes.length === 0) return;
    if (modeWatchRef.current?.term !== term) {
      modeWatchRef.current?.subscription.dispose();
      const subscription = term.onWriteParsed?.(() => setLiveTuiOwnsNavigation(tuiOwnsNavigation(term)));
      modeWatchRef.current = subscription ? { term, subscription } : null;
    }
    traceModes(term, modes);
    setLiveTuiOwnsNavigation(tuiOwnsNavigation(term));
  };

  // Whether this terminal can page back at all — the feed's answer, not the transport's. This is
  // the whole of what gives a LiveKit-carried session scrollback: the same question, asked of the
  // connection rather than of the component that happened to be rendering.
  const canScrollBack = feedSupportsHistory(feed);
  const historyRef = useRef(feed.history);
  historyRef.current = feed.history;

  const startForwardFill = async () => {
    if (fillingRef.current || filled) return;
    const fetcher = historyRef.current;
    if (!fetcher) return;
    const a = anchorRef.current;
    // `a.atOldest` (captured from the initial replay frame) gates the fill: if the ring was already
    // at its oldest at open time, there is no older history to load.
    if (a?.atOldest) return;
    // The forward-fill upper bound is the CURRENT live tip (`currentOffset`), NOT the stale anchor
    // `endOffset`: the capture ring may have evicted the original tip, in which case
    // `replay_from(0, anchor)` is an empty range and the page would swap to blank. Bounding by the
    // current tip yields `[start_offset, tip]` — the full retained history at fill time.
    //
    // `null` where no anchor was ever captured: no frame has reported an offset past zero. There
    // the fill runs to the capture tip and ends on the chunk's own `atEnd` — see
    // `TerminalHistoryForwardLoader`.
    const untilOffset = a === null ? null : currentOffsetRef.current;
    if (untilOffset !== null && untilOffset <= 0n) return;
    fillingRef.current = true;
    setLoading(true);
    if (loaderRef.current === null) {
      loaderRef.current = new TerminalHistoryForwardLoader(untilOffset, a?.atOldest ?? false);
    }
    const loader = loaderRef.current;
    let wroteAny = false;
    let failed = false;
    try {
      while (!loader.done) {
        const chunk = await loader.loadNext(fetcher);
        if (chunk === null) break;
        if (chunk.data.length > 0) {
          wroteAny = true;
          const older = olderTermRef.current;
          if (olderReadyRef.current && older) {
            older.write(chunk.data);
          } else {
            olderBufferRef.current.push(chunk.data);
          }
          dHistory(
            "forward-filled chunk startOffset=%s endOffset=%s atEnd=%o bytes=%d",
            chunk.startOffset.toString(),
            chunk.endOffset.toString(),
            chunk.atEnd,
            chunk.data.length,
          );
        }
      }
    } catch (err) {
      // A failed fetch (e.g. `getTerminalHistory` not supported for this session type, or a transport
      // error) must NOT swap to a blank page — stay on the live pane so the user keeps their terminal.
      failed = true;
      dHistory(
        "forward-fill failed (staying on live): %s",
        err instanceof Error ? err.message : String(err),
      );
    } finally {
      fillingRef.current = false;
      setLoading(false);
      // Only swap to the history page when the fill actually produced history. A failed fetch or an
      // empty retained range leaves the user on the live pane (the affordance stays available so the
      // fill can be retried once history is available) instead of swapping to a blank page.
      if (!failed && wroteAny) {
        setFilled(true);
        // Swap the page terminal to the foreground and land at its bottom (newest pre-anchor line).
        // Synchronous (not rAF-deferred) so a subsequent programmatic scroll can't race with a pending
        // landing scroll and get clobbered back to the bottom.
        setView("page");
        olderTermRef.current?.scrollToBottom?.();
      }
      refreshMirrors();
    }
  };

  // Swap back to the live terminal (instant — it has been streaming underneath, so it is current).
  const backToLive = () => {
    setView("live");
  };

  // Swap to the history page terminal instantly (no re-fetch) once it has already been filled.
  const viewHistory = () => {
    if (!filled) return;
    setView("page");
    olderTermRef.current?.scrollToBottom?.();
  };

  // The "Load earlier output" affordance is shown on the live pane before the first fill (older
  // history can be served and is not yet loaded). After the first fill, the live pane shows a
  // "View history" affordance instead (instant swap, no fetch). The anchor only ever *withdraws*
  // the offer — a wire that states `atOldest` has nothing older — because a wire that states no
  // anchor at all is not a wire without history; it is one whose history the connection fetches
  // from somewhere else. Neither is offered while a full-screen TUI owns navigation: it scrolls its
  // own output, and its alternate-screen repaints never become scrollback, so the history replay
  // would hold frames rather than a transcript. "Back to live" is never withheld.
  const historyOffered = canScrollBack && !liveTuiOwnsNavigation;
  const loadEarlierVisible = historyOffered && !anchor?.atOldest && !filled && !loading;
  const viewHistoryVisible = historyOffered && filled && view === "live";
  const backToLiveVisible = view === "page";

  return {
    canScrollBack,
    anchorRef,
    setAnchor,
    view,
    loading,
    filled,
    noteLiveModeSwitches,
    startForwardFill,
    backToLive,
    viewHistory,
    loadEarlierVisible,
    viewHistoryVisible,
    backToLiveVisible,
  };
}
