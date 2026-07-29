import React, { useEffect, useRef, useState } from "react";
import { DEFAULT_TERMINAL_FONT_MAX, DEFAULT_TERMINAL_FONT_MIN } from "../lib/terminalZoom";
import { tddyDebug } from "../lib/debugMask";
import { GhosttyTerminal, type GhosttyTerminalHandle } from "./GhosttyTerminal";
import { ConnectionTerminalChrome } from "./connection/ConnectionTerminalChrome";
import { TerminalConnectionStatusBar } from "./connection/TerminalConnectionStatusBar";
import { MobileTerminalKeyboard } from "./connection/MobileTerminalKeyboard";
import { TerminalFileDropZone } from "./connection/TerminalFileDropZone";
import { TerminalUploadButton } from "./connection/TerminalUploadButton";
import { ShortcutDrawer } from "./connection/ShortcutDrawer";
import { useIsMobile } from "../hooks/useIsMobile";
import { useVisualViewport } from "../hooks/useVisualViewport";
import type { ToolShortcutDef } from "../lib/toolShortcuts";
import {
  TerminalHistoryForwardLoader,
  type HistoryChunk,
} from "../lib/terminalHistoryLoader";

// `[tddy]` diagnostics for the gRPC terminal byte stream (enabled by the DEBUG mask).
// The 220-col garbling on reconnect lived here, so log incoming bytes / buffering / resize.
const dGrpc = tddyDebug("tddy:term:grpc");
const dResize = tddyDebug("tddy:term:resize");
const dHistory = tddyDebug("tddy:term:history");

/** Hex preview of the first `n` bytes for diagnosing garbled / misaligned output. */
function hexPreview(data: Uint8Array, n = 24): string {
  return Array.from(data.slice(0, n), (b) => b.toString(16).padStart(2, "0")).join(" ");
}

/** Scrollback lines retained by the older-history page terminal (large enough to hold a full session). */
const PAGE_SCROLLBACK = 50000;

/**
 * One frame from `StreamTerminalOutput`: the raw output bytes plus the offset metadata carried on
 * the initial replay frame. Live tail frames leave `endOffset`/`atOldest` at their zero defaults.
 */
export interface GrpcFrame {
  data: Uint8Array;
  endOffset: bigint;
  atOldest: boolean;
}

export interface GrpcStream {
  send(data: Uint8Array): void;
  onMessage(fn: (frame: GrpcFrame) => void): void;
  close(): void;
}

/**
 * Fetches one forward chunk of older history starting at `fromOffset`, bounded by `untilOffset`
 * (the anchor). Returns `null` when the backend has no chunk for the range.
 */
export type HistoryFetcher = (
  fromOffset: bigint,
  untilOffset: bigint,
) => Promise<HistoryChunk | null>;

export interface GhosttyTerminalGrpcProps {
  sessionToken: string;
  sessionId: string;
  stream: GrpcStream;
  connectionOverlay?: boolean;
  onDisconnect?: () => void;
  fontSize?: number;
  minFontSize?: number;
  maxFontSize?: number;
  /** Shortcut presets — on mobile, rendered as the draggable ShortcutDrawer overlay. */
  mobileShortcuts?: ToolShortcutDef[];
  /** Called with a function that types text into the terminal input (no newline). Lets the session
   *  runtime expose this terminal's insert to the inspector's Files tab (click/tap route). */
  onRegisterInsertInput?: (insertInput: (text: string) => void) => void;
  /** Forward-fill fetcher for the older-history page terminal. When provided, the component overlays
   *  a second, read-only ghostty-web terminal behind the live one. On a scroll-up-at-top gesture
   *  (or the "Load earlier output" affordance) the page terminal is forward-filled in the background
   *  while a loading indicator is shown; once the fill completes the two terminals switch places
   *  (the page terminal becomes foreground, scrollable through history; the live terminal stays
   *  mounted underneath and keeps receiving the stream). "Back to live" (or a scroll-down-at-bottom
   *  gesture on the page terminal) swaps back. All paging logic is encapsulated here. */
  historyFetcher?: HistoryFetcher;
  /** Called per output frame with the cumulative byte offset the client has now received up to.
   *  On a replay / catch-up frame (`endOffset > 0`) the offset is the frame's absolute `endOffset`;
   *  on a live tail frame (`endOffset === 0`) the offset advances by the frame's byte length. The
   *  parent uses this to track `currentOffset` so a reconnect can resume with `FROM_OFFSET` instead
   *  of re-replaying (no duplicates). */
  onOffsetUpdate?: (offset: bigint) => void;
}

export function GhosttyTerminalGrpc({
  sessionToken,
  sessionId,
  stream,
  connectionOverlay,
  onDisconnect,
  fontSize = 14,
  minFontSize = DEFAULT_TERMINAL_FONT_MIN,
  maxFontSize = DEFAULT_TERMINAL_FONT_MAX,
  mobileShortcuts,
  onRegisterInsertInput,
  historyFetcher,
  onOffsetUpdate,
}: GhosttyTerminalGrpcProps) {
  const termRef = useRef<GhosttyTerminalHandle>(null);
  const olderTermRef = useRef<GhosttyTerminalHandle>(null);
  const liveContainerRef = useRef<HTMLDivElement>(null);
  const pageContainerRef = useRef<HTMLDivElement>(null);
  const olderReadyRef = useRef(false);
  const olderBufferRef = useRef<Uint8Array[]>([]);
  const termReadyRef = useRef(false);
  const outputBufferRef = useRef<Uint8Array[]>([]);
  const [bufferText, setBufferText] = useState("");
  const [olderBufferText, setOlderBufferText] = useState("");
  // Viewport position mirror for the page terminal (lines up from the bottom). Surfaced through a
  // hidden element so component tests can assert scrollToLine gives full control of the viewport.
  const [pageViewportY, setPageViewportY] = useState(0);
  const [liveViewportY, setLiveViewportY] = useState(0);
  const [liveScrollbackLength, setLiveScrollbackLength] = useState(0);
  const [pageScrollbar, setPageScrollbar] = useState("");
  const [liveScrollbar, setLiveScrollbar] = useState("");
  const isMobile = useIsMobile();
  const { isKeyboardOpen } = useVisualViewport();

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
  const historyFetcherRef = useRef(historyFetcher);
  historyFetcherRef.current = historyFetcher;

  // Cumulative output offset the client has received up to. On a replay / catch-up frame
  // (`endOffset > 0`) it snaps to that absolute offset; on a live tail frame it advances by the
  // frame's byte length. Surfaced to the parent via `onOffsetUpdate` so a reconnect can resume with
  // `FROM_OFFSET` instead of re-replaying (no duplicates).
  const currentOffsetRef = useRef(0n);
  const onOffsetUpdateRef = useRef(onOffsetUpdate);
  onOffsetUpdateRef.current = onOffsetUpdate;

  const refreshMirrors = () => {
    setPageViewportY(olderTermRef.current?.getViewportScrollOffset?.() ?? 0);
    setLiveViewportY(termRef.current?.getViewportScrollOffset?.() ?? 0);
    setLiveScrollbackLength(termRef.current?.getScrollbackLength?.() ?? 0);
    const pageSb = olderTermRef.current?.getScrollbar?.();
    setPageScrollbar(pageSb ? `${pageSb.total},${pageSb.offset},${pageSb.len}` : "");
    const liveSb = termRef.current?.getScrollbar?.();
    setLiveScrollbar(liveSb ? `${liveSb.total},${liveSb.offset},${liveSb.len}` : "");
  };

  const sendInput = (data: string | Uint8Array) =>
    stream.send(typeof data === "string" ? new TextEncoder().encode(data) : data);

  // Expose text-insert to the runtime (for the inspector's Files-tab click/tap route). A ref keeps
  // the registered function current without re-registering every render.
  const sendInputRef = useRef(sendInput);
  sendInputRef.current = sendInput;
  useEffect(() => {
    onRegisterInsertInput?.((text: string) => sendInputRef.current(text));
  }, [onRegisterInsertInput]);

  useEffect(() => {
    stream.onMessage((frame) => {
      const data = frame.data;
      // Capture the lazy-history anchor from the initial replay frame (endOffset > 0). The anchor
      // drives the forward fill of the older-history terminal; storing it in state ensures the
      // affordance re-renders immediately when it is captured.
      if (frame.endOffset > 0n && anchorRef.current === null) {
        const a = { endOffset: frame.endOffset, atOldest: frame.atOldest };
        anchorRef.current = a;
        setAnchor(a);
        dHistory(
          "lazy history anchor endOffset=%s atOldest=%o sessionId=%s",
          frame.endOffset.toString(),
          frame.atOldest,
          sessionId,
        );
      }
      // Advance the cumulative output offset: a replay / catch-up frame snaps to its absolute
      // `endOffset`; a live tail frame advances by its byte length. Surface to the parent so a
      // reconnect resumes with `FROM_OFFSET` (no duplicate replay).
      currentOffsetRef.current =
        frame.endOffset > 0n
          ? frame.endOffset
          : currentOffsetRef.current + BigInt(data.length);
      onOffsetUpdateRef.current?.(currentOffsetRef.current);
      const ready = termReadyRef.current && !!termRef.current;
      if (dGrpc.enabled) {
        dGrpc("recv %d bytes ready=%o %s", data.length, ready, hexPreview(data));
      }
      if (ready && termRef.current) {
        termRef.current.write(data);
      } else {
        outputBufferRef.current.push(data);
      }
    });
    return () => {
      stream.close();
    };
  }, [stream, sessionId]);

  useEffect(() => {
    const interval = setInterval(() => {
      const text = termRef.current?.getBufferText?.() ?? "";
      setBufferText(text);
      const olderText = olderTermRef.current?.getBufferText?.() ?? "";
      setOlderBufferText(olderText);
      refreshMirrors();
    }, 200);
    return () => clearInterval(interval);
  }, []);

  // Drive the progressive forward fill of the older-history page terminal in the background: append
  // one forward chunk at a time (oldest→anchor) until the loader reports done. While filling, the
  // loading indicator is shown over the foreground terminal; on completion the page terminal swaps
  // to the foreground (landed at its bottom = the newest pre-anchor line, seamless). Live bytes keep
  // flowing to the live terminal independently — no buffering, no reset.
  const startForwardFill = async () => {
    if (fillingRef.current || filled) return;
    const fetcher = historyFetcherRef.current;
    const a = anchorRef.current;
    // `a.atOldest` (captured from the initial replay frame) gates the affordance: if the ring was
    // already at its oldest at open time, there is no older history to load. The forward-fill upper
    // bound, though, is the CURRENT live tip (`currentOffset`), NOT the stale anchor `endOffset`:
    // the capture ring may have evicted the original tip, in which case `replay_from(0, anchor)` is
    // an empty range and the page would swap to blank. Bounding by the current tip yields
    // `[start_offset, tip]` — the full retained history at fill time — which is always non-empty
    // while any history is retained (and the affordance is hidden when none is).
    if (!fetcher || !a || a.atOldest) return;
    const untilOffset = currentOffsetRef.current;
    if (untilOffset <= 0n) return;
    fillingRef.current = true;
    setLoading(true);
    if (loaderRef.current === null) {
      loaderRef.current = new TerminalHistoryForwardLoader(untilOffset, a.atOldest);
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

  // FIXME: test-only hooks; remove if real wheel/keystroke drivers become available in Cypress.
  useEffect(() => {
    const win = window as unknown as {
      __tddyPageScrollUp?: (n: number) => void;
      __tddyPageScrollToLine?: (n: number) => void;
      __tddyLiveMouseTracking?: (on: boolean) => void;
    };
    win.__tddyPageScrollUp = (n: number) => {
      olderTermRef.current?.scrollLines?.(-n);
    };
    win.__tddyPageScrollToLine = (n: number) => {
      olderTermRef.current?.scrollToLine?.(n);
    };
    win.__tddyLiveMouseTracking = (on: boolean) => {
      if (on) {
        termRef.current?.write("\x1b[?1006h\x1b[?1002h");
      } else {
        termRef.current?.write("\x1b[?1002l\x1b[?1006l");
      }
    };
    return () => {
      delete win.__tddyPageScrollUp;
      delete win.__tddyPageScrollToLine;
      delete win.__tddyLiveMouseTracking;
    };
  }, []);

  // Three-way wheel gate on the live pane (capture phase — runs before ghostty-web's canvas handler):
  // 1. Mouse tracking ON → SGR wheel report to the TUI; block ghostty-web arrow emulation.
  // 2. Mouse tracking OFF + alternate screen → no-op here; ghostty-web emits Up/Down for pagers.
  // 3. Mouse tracking OFF + normal screen → wheel-up triggers forward-fill (or instant page swap).
  useEffect(() => {
    const el = liveContainerRef.current;
    if (!el || !historyFetcher) return;
    const handler = (e: WheelEvent) => {
      const mouseTracking = termRef.current?.hasMouseTracking?.() ?? false;
      const alternateScreen = termRef.current?.isAlternateScreen?.() ?? false;

      if (mouseTracking) {
        if (!e.ctrlKey) {
          termRef.current?.sendWheelSgr?.(e);
        }
        e.preventDefault();
        e.stopPropagation();
        return;
      }

      if (alternateScreen) {
        return;
      }

      if (e.deltaY >= 0) return;
      if (filled) {
        setView("page");
        olderTermRef.current?.scrollToBottom?.();
        return;
      }
      // scrollback 0 ⇒ always pinned to the bottom ⇒ any wheel-up requests older history.
      if (termRef.current?.isPinnedToBottom?.() ?? true) {
        void startForwardFill();
      }
    };
    el.addEventListener("wheel", handler, { capture: true });
    return () => el.removeEventListener("wheel", handler, { capture: true } as AddEventListenerOptions);
  }, [historyFetcher, filled]);

  // Scroll-down-at-bottom gesture on the PAGE terminal: when the user scrolls down while pinned to
  // the bottom of the older-history page, swap back to the live terminal (seamless return to the
  // live tip). Capture phase for the same reason as the live-pane listener.
  useEffect(() => {
    const el = pageContainerRef.current;
    if (!el || !historyFetcher) return;
    const handler = (e: WheelEvent) => {
      if (e.deltaY <= 0) return;
      if (olderTermRef.current?.isPinnedToBottom?.() ?? false) {
        setView("live");
      }
    };
    el.addEventListener("wheel", handler, { capture: true });
    return () => el.removeEventListener("wheel", handler, { capture: true } as AddEventListenerOptions);
  }, [historyFetcher]);

  // The "Load earlier output" affordance is shown on the live pane before the first fill (older
  // history is available and not yet loaded). After the first fill, the live pane shows a
  // "View history" affordance instead (instant swap, no fetch).
  const loadEarlierVisible =
    !!historyFetcher && !!anchor && !anchor.atOldest && !filled && !loading;
  const viewHistoryVisible =
    !!historyFetcher && !!anchor && filled && view === "live";
  const backToLiveVisible = view === "page";

  const terminal = (
    <GhosttyTerminal
      ref={termRef}
      fontSize={fontSize}
      minFontSize={minFontSize}
      maxFontSize={maxFontSize}
      preventFocusOnTap={isMobile && !isKeyboardOpen}
      onReady={() => {
        termReadyRef.current = true;
        const buf = outputBufferRef.current;
        outputBufferRef.current = [];
        const term = termRef.current;
        if (term) {
          dGrpc("ready — flushing %d buffered chunk(s)", buf.length);
          for (const chunk of buf) {
            term.write(chunk);
          }
          // On mobile, don't auto-focus (would pop the soft keyboard / fight the
          // mobile keyboard affordance); the user opens it via the Keyboard button.
          if (!isMobile) {
            term.focus();
          }
        }
      }}
      onData={(data) => {
        sendInput(data);
      }}
      onResize={(size) => {
        dResize("OSC resize send cols=%d rows=%d seq=\\x1b]resize;%d;%d\\x07", size.cols, size.rows, size.cols, size.rows);
        sendInput(`\x1b]resize;${size.cols};${size.rows}\x07`);
      }}
    />
  );

  const olderTerminal = (
    <GhosttyTerminal
      ref={olderTermRef}
      fontSize={fontSize}
      minFontSize={minFontSize}
      maxFontSize={maxFontSize}
      scrollback={PAGE_SCROLLBACK}
      testId="ghostty-terminal-older"
      preventFocusOnTap
      onReady={() => {
        olderReadyRef.current = true;
        const buf = olderBufferRef.current;
        olderBufferRef.current = [];
        const term = olderTermRef.current;
        if (term) {
          for (const chunk of buf) {
            term.write(chunk);
          }
        }
        refreshMirrors();
      }}
      onScroll={() => {
        refreshMirrors();
      }}
    />
  );

  const liveForeground = view === "live";
  const affordanceBtnStyle: React.CSSProperties = {
    position: "absolute",
    top: 4,
    right: 8,
    zIndex: 5,
    fontSize: 11,
    padding: "2px 8px",
    background: "var(--muted, #2a2b3d)",
    color: "var(--muted-foreground, #a9b1d6)",
    border: "1px solid var(--border, #333)",
    borderRadius: 4,
    cursor: "pointer",
  };

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        width: "100%",
        minWidth: 0,
        minHeight: 0,
        height: "100%",
      }}
    >
      {connectionOverlay ? (
        <TerminalConnectionStatusBar className="border-b border-border bg-muted">
          <ConnectionTerminalChrome
            chromeLayout="statusBar"
            overlayStatus="connected"
            onDisconnect={onDisconnect ?? (() => {})}
            statusBarShowFullscreen={false}
            statusBarShowBuildId={false}
          />
        </TerminalConnectionStatusBar>
      ) : null}
      <div style={{ flex: 1, minHeight: 0, minWidth: 0, width: "100%", position: "relative" }}>
        {/* Live pane: scrollback 0 (always pinned to the live tip), always mounted, always receives
            the stream. Foreground at the tip; stays mounted underneath while browsing history. */}
        <div
          ref={liveContainerRef}
          data-testid="terminal-live-pane"
          data-foreground={liveForeground ? "true" : "false"}
          style={{
            position: "absolute",
            inset: 0,
            zIndex: liveForeground ? 2 : 1,
            visibility: liveForeground ? "visible" : "hidden",
            pointerEvents: liveForeground ? "auto" : "none",
          }}
        >
          {loadEarlierVisible ? (
            <button
              type="button"
              data-testid="load-earlier-history"
              onClick={() => {
                void startForwardFill();
              }}
              style={affordanceBtnStyle}
            >
              Load earlier output
            </button>
          ) : null}
          {viewHistoryVisible ? (
            <button
              type="button"
              data-testid="view-history"
              onClick={viewHistory}
              style={affordanceBtnStyle}
            >
              View history
            </button>
          ) : null}
          <TerminalFileDropZone
            sessionToken={sessionToken}
            sessionId={sessionId}
            insertInput={sendInput}
          >
            {terminal}
          </TerminalFileDropZone>
          {isMobile && mobileShortcuts && mobileShortcuts.length > 0 && (
            <ShortcutDrawer shortcuts={mobileShortcuts} onSend={sendInput} />
          )}
        </div>
        {/* Page pane: scrollback > 0, holds the forward-filled older history. Background (hidden)
            while being filled; swaps to the foreground once the fill completes. Hidden entirely
            (not unmounted) so its scrollback survives across swaps. */}
        {historyFetcher ? (
          <div
            ref={pageContainerRef}
            data-testid="terminal-page-pane"
            data-foreground={!liveForeground ? "true" : "false"}
            style={{
              position: "absolute",
              inset: 0,
              zIndex: liveForeground ? 1 : 2,
              visibility: liveForeground ? "hidden" : "visible",
              pointerEvents: liveForeground ? "none" : "auto",
            }}
          >
            {backToLiveVisible ? (
              <button
                type="button"
                data-testid="back-to-live"
                onClick={backToLive}
                style={affordanceBtnStyle}
              >
                Back to live
              </button>
            ) : null}
            {olderTerminal}
          </div>
        ) : null}
        {/* Loading indicator: shown over whichever pane is foreground while the background page
            terminal is being forward-filled. */}
        {loading ? (
          <div
            data-testid="terminal-history-loading"
            style={{
              position: "absolute",
              top: 6,
              left: "50%",
              transform: "translateX(-50%)",
              zIndex: 6,
              fontSize: 11,
              padding: "2px 10px",
              background: "var(--muted, #2a2b3d)",
              color: "var(--muted-foreground, #a9b1d6)",
              border: "1px solid var(--border, #333)",
              borderRadius: 4,
              pointerEvents: "none",
            }}
          >
            Loading history…
          </div>
        ) : null}
      </div>
      {isMobile && (
        <div className="flex-shrink-0 flex items-center justify-center gap-2 border-t border-border bg-muted p-1">
          <MobileTerminalKeyboard onSend={sendInput} />
          <TerminalUploadButton
            sessionToken={sessionToken}
            sessionId={sessionId}
            insertInput={sendInput}
          />
        </div>
      )}
      <div data-testid="terminal-buffer-text" style={{ display: "none" }} aria-hidden>
        {bufferText}
      </div>
      <div data-testid="terminal-older-buffer-text" style={{ display: "none" }} aria-hidden>
        {olderBufferText}
      </div>
      <div data-testid="terminal-page-viewport-y" style={{ display: "none" }} aria-hidden>
        {pageViewportY}
      </div>
      <div data-testid="terminal-live-viewport-y" style={{ display: "none" }} aria-hidden>
        {liveViewportY}
      </div>
      <div data-testid="terminal-live-scrollback-length" style={{ display: "none" }} aria-hidden>
        {liveScrollbackLength}
      </div>
      <div data-testid="terminal-page-scrollbar" style={{ display: "none" }} aria-hidden>
        {pageScrollbar}
      </div>
      <div data-testid="terminal-live-scrollbar" style={{ display: "none" }} aria-hidden>
        {liveScrollbar}
      </div>
    </div>
  );
}
