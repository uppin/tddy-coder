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
 * Scrollback retained by the LIVE terminal. Both terminals are scrollable so their viewports can be
 * synced (mirroring viewportY across the overlay pair keeps a swap from jumping in scroll position).
 * Tradeoff accepted by design: a scrollback>0 live terminal accumulates duplicate panes from periodic
 * TUI full-screen re-paints — ghostty-web has no prepend/suppress API to avoid it.
 */
const LIVE_SCROLLBACK = PAGE_SCROLLBACK;

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
  // Viewport position mirrors for testability: each terminal's viewportY (lines up from the
  // bottom) is polled and surfaced through a hidden element so component tests can assert that
  // scrolling the foreground terminal mirrors onto the background terminal (the sync contract).
  const [liveViewportY, setLiveViewportY] = useState(0);
  const [pageViewportY, setPageViewportY] = useState(0);
  const isMobile = useIsMobile();
  const { isKeyboardOpen } = useVisualViewport();

  // Overlay double-buffer paging state. Two ghostty-web terminals share the same rect; `view`
  // decides which is foreground (visible, interactive). The live terminal (scrollback 0) always
  // stays mounted and keeps receiving the stream, so swapping back to "live" is instant and current.
  // `loading` shows the loading indicator while the background page terminal is forward-filled;
  // `filled` records that the page terminal has been populated (enables instant re-swap to history).
  const loaderRef = useRef<TerminalHistoryForwardLoader | null>(null);
  const anchorRef = useRef<{ endOffset: bigint; atOldest: boolean } | null>(null);
  const [anchor, setAnchor] = useState<{ endOffset: bigint; atOldest: boolean } | null>(null);
  const fillingRef = useRef(false);
  const [view, setView] = useState<"live" | "page">("live");
  const [loading, setLoading] = useState(false);
  const [filled, setFilled] = useState(false);
  const historyFetcherRef = useRef(historyFetcher);
  historyFetcherRef.current = historyFetcher;

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
      setLiveViewportY(termRef.current?.getViewportScrollOffset?.() ?? 0);
      setPageViewportY(olderTermRef.current?.getViewportScrollOffset?.() ?? 0);
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
    if (!fetcher || !a || a.atOldest || a.endOffset <= 0n) return;
    fillingRef.current = true;
    setLoading(true);
    if (loaderRef.current === null) {
      loaderRef.current = new TerminalHistoryForwardLoader(a.endOffset, a.atOldest);
    }
    const loader = loaderRef.current;
    try {
      while (!loader.done) {
        const chunk = await loader.loadNext(fetcher);
        if (chunk === null) break;
        if (chunk.data.length > 0) {
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
    } finally {
      fillingRef.current = false;
      setLoading(false);
      setFilled(true);
      // Swap the page terminal to the foreground and land at its bottom (newest pre-anchor line).
      // Synchronous (not rAF-deferred) so a subsequent programmatic scroll can't race with a pending
      // landing scroll and get clobbered back to the bottom.
      setView("page");
      olderTermRef.current?.scrollToBottom?.();
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

  // Viewport sync across the overlay pair. Both terminals are scrollable; when the FOREGROUND
  // terminal is scrolled by the user, mirror its viewportY (lines up from the bottom) onto the
  // background terminal by a relative scrollLines delta (current viewportY → target viewportY).
  // Relative mirroring avoids the absolute-line coordinate ambiguity of scrollToLine and is robust
  // to differing buffer lengths. Because the background pane is hidden (pointer-events none,
  // visibility hidden) the user cannot scroll it, so any onScroll it fires is from our own
  // programmatic scrollLines — the foreground-only guard below absorbs that and prevents a
  // feedback loop without needing a re-entrancy flag. The two buffers differ in content
  // (live = recent output, page = older history), so this syncs the scroll OFFSET, not the content;
  // at the seam (viewportY 0) the two are contiguous by construction, so a swap there is seamless.
  const mirrorViewport = (source: "live" | "page", vy: number) => {
    const target = source === "live" ? olderTermRef.current : termRef.current;
    if (!target) return;
    const cur = target.getViewportScrollOffset?.() ?? 0;
    const delta = vy - cur;
    if (delta !== 0) target.scrollLines?.(-delta);
  };
  const onLiveScroll = (vy: number) => {
    if (view === "live") mirrorViewport("live", vy);
  };
  const onPageScroll = (vy: number) => {
    if (view === "page") mirrorViewport("page", vy);
  };

  // Test-only hook: scroll the FOREGROUND terminal up by `n` lines via its imperative handle. Real
  // wheel events don't reach ghostty-web reliably under Cypress, so component tests drive the
  // viewport through this hook to exercise the onScroll→scrollToLine sync path deterministically.
  // The foreground is selected by `view`. Marked FIXME: test-only — must not be relied on in prod.
  useEffect(() => {
    // FIXME: test-only hook; remove if a real wheel-driver becomes available in Cypress.
    const win = window as unknown as { __tddyScrollForegroundUp?: (n: number) => void };
    win.__tddyScrollForegroundUp = (n: number) => {
      if (view === "live") {
        termRef.current?.scrollLines?.(-n);
      } else {
        olderTermRef.current?.scrollLines?.(-n);
      }
    };
    return () => {
      delete win.__tddyScrollForegroundUp;
    };
  }, [view]);

  // Scroll-up-at-top gesture on the LIVE terminal: the live terminal has scrollback 0 (always pinned
  // to bottom), so a wheel-up attempt is interpreted as "show older history". On the first activation
  // it starts the forward fill; once the page terminal is filled, the same gesture swaps to it
  // instantly. The listener is attached in the CAPTURE phase so it fires before ghostty-web's own
  // wheel handler (which may stop propagation); a React `onWheel` (bubble phase) would never see it.
  useEffect(() => {
    const el = liveContainerRef.current;
    if (!el || !historyFetcher) return;
    const handler = (e: WheelEvent) => {
      if (e.deltaY >= 0) return;
      if (filled) {
        setView("page");
        olderTermRef.current?.scrollToBottom?.();
        return;
      }
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
      scrollback={LIVE_SCROLLBACK}
      onScroll={onLiveScroll}
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
      onScroll={onPageScroll}
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
        {/* Live pane: scrollback > 0 (synced with the page terminal), always mounted, always
            receives the stream. Foreground at the live tip; stays mounted underneath (hidden)
            while browsing history so it stays current. */}
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
      <div data-testid="terminal-live-viewport-y" style={{ display: "none" }} aria-hidden>
        {liveViewportY}
      </div>
      <div data-testid="terminal-page-viewport-y" style={{ display: "none" }} aria-hidden>
        {pageViewportY}
      </div>
    </div>
  );
}
