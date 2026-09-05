/**
 * One terminal, fed by a session's connection.
 *
 * This is the LiveKit terminal and the gRPC terminal merged. They rendered the same chrome
 * — `GhosttyTerminal`, `ConnectionTerminalChrome`, `TerminalConnectionStatusBar`, `ShortcutDrawer`,
 * `MobileTerminalKeyboard`, `TerminalFileDropZone`, `TerminalUploadButton` — and differed in two
 * things: how bytes arrived, and what each could do. Node 3 of the `optional-livekit` stack moved
 * the room, the identity and the token onto the `SessionConnection`, which left the first
 * difference with no cause; this component removes the second, because scrollback now follows the
 * {@link TerminalFeed} rather than the wire.
 *
 * It therefore imports no `livekit-client`, constructs no `Room` and mints no token. What it is
 * handed is a feed: bytes to render, bytes to send, and — where the connection can serve it —
 * history to page back through. A LiveKit-carried session has that history for the first time.
 *
 * PRD: `docs/dev/1-WIP/2026-09-05-optional-livekit-terminal-convergence-prd.md`.
 * Feature: `docs/ft/web/web-terminal.md`, `docs/ft/web/terminal-replay-lazy-scroll.md`.
 */

import React, { useEffect, useRef, useState } from "react";
import { DEFAULT_TERMINAL_FONT_MAX, DEFAULT_TERMINAL_FONT_MIN } from "../lib/terminalZoom";
import { tddyDebug } from "../lib/debugMask";
import { tddyDevDebug } from "../lib/tddyDevLog";
import {
  shouldShowVisibleLiveKitStatusStrip,
  type LiveKitChromeStatus,
} from "../lib/liveKitStatusPresentation";
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
import { TerminalHistoryForwardLoader } from "../lib/terminalHistoryLoader";
import { TerminalStreamOffset } from "../lib/terminalStreamOffset";
import { feedSupportsHistory, type TerminalFeed } from "../rpc/connections/terminal";
import type { ByteDelta } from "./sessions/sessionRuntimeRegistry";

// `[tddy]` diagnostics for the terminal byte stream (enabled by the DEBUG mask).
// The 220-col garbling on reconnect lived here, so log incoming bytes / buffering / resize.
const dTerm = tddyDebug("tddy:term:grpc");
const dResize = tddyDebug("tddy:term:resize");
const dHistory = tddyDebug("tddy:term:history");

/** Hex preview of the first `n` bytes for diagnosing garbled / misaligned output. */
function hexPreview(data: Uint8Array, n = 24): string {
  return Array.from(data.slice(0, n), (b) => b.toString(16).padStart(2, "0")).join(" ");
}

/** Scrollback lines retained by the older-history page terminal (large enough to hold a full session). */
const PAGE_SCROLLBACK = 50000;

/** Human-readable description of a terminal input byte sequence, for `debugLogging`. */
export function describeKey(bytes: Uint8Array): string {
  if (bytes.length === 1) {
    const b = bytes[0];
    if (b === 0x0d) return "Enter";
    if (b === 0x1b) return "Esc";
    if (b === 0x09) return "Tab";
    if (b === 0x7f) return "Backspace";
    if (b === 0x03) return "Ctrl+C";
    if (b < 0x20) return `Ctrl+${String.fromCharCode(b + 0x40)}`;
    if (b >= 0x20 && b < 0x7f) return `'${String.fromCharCode(b)}'`;
  }
  if (bytes.length === 3 && bytes[0] === 0x1b && bytes[1] === 0x5b) {
    const c = bytes[2];
    if (c === 0x41) return "Up";
    if (c === 0x42) return "Down";
    if (c === 0x43) return "Right";
    if (c === 0x44) return "Left";
  }
  if (bytes.length === 4 && bytes[0] === 0x1b && bytes[1] === 0x5b && bytes[3] === 0x7e) {
    if (bytes[2] === 0x35) return "PageUp";
    if (bytes[2] === 0x36) return "PageDown";
  }
  return `raw(${bytes.length})`;
}

export interface GhosttyTerminalSessionProps {
  /**
   * The session's terminal: its byte stream, its scrollback where the connection can serve one, and
   * the end of it. Opened by the caller off a `SessionConnection` (`openTerminal`), never built
   * here — which is what makes this component the same one on every wire.
   */
  feed: TerminalFeed;

  /**
   * Daemon session token + id for the file drop / upload affordances. When both are set, files
   * dropped on the terminal (or picked via the mobile Attach button) upload to the session dir and
   * their host paths are typed in. See `docs/ft/web/web-terminal.md` § File drop upload.
   */
  sessionToken?: string;
  sessionId?: string;

  /**
   * How the connection carrying {@link feed} is doing, when the caller has something to say about
   * it. Drives the status dot and the raw `livekit-status` readout.
   *
   * Omitted by a caller whose connection state is shown elsewhere — the sessions drawer covers its
   * panes with `SessionConnectionOverlay` — in which case this component claims nothing about it
   * and the readout stays out of the layout.
   */
  connectionStatus?: LiveKitChromeStatus;

  /** Why the connection failed, shown over the terminal while {@link connectionStatus} is `error`. */
  connectionError?: string;

  /** When set, show connection chrome (status dot menu). Interrupt is the TUI Stop pane (SGR mouse → 0x03). */
  connectionOverlay?: {
    onDisconnect: () => void;
    buildId?: string;
    /** When set, the dot menu includes Terminate (daemon session SIGTERM path). */
    onTerminate?: () => void;
  };

  /**
   * Maps to `ConnectionTerminalChrome` `chromeLayout="statusBar"`:
   *
   * | `connectionChromePlacement` | Effect on status bar |
   * |------------------------------|----------------------|
   * | `"floating"` (default)       | Full bar: build id, dot, fullscreen, optional mobile keyboard slot |
   * | `"none"`                     | Compact bar: dot + menu (+ mobile keyboard); no build id / fullscreen (overlay panes) |
   */
  connectionChromePlacement?: "floating" | "none";

  /** Initial terminal font size (session baseline for Ctrl/⌘+0 reset). Default 14. */
  fontSize?: number;
  /** Bounds for zoom / pinch; default min 8, max 32. Overlay may pass a lower min (e.g. 2px). */
  minFontSize?: number;
  maxFontSize?: number;

  /** Fixed logical grid; font scales with the container (floating overlay). */
  fixedViewportGrid?: { cols: number; rows: number };
  /** Passed to `GhosttyTerminal` — use `0` for fixed-height overlay panes (default 200px min is too tall). */
  terminalContainerMinHeightPx?: number;
  /** When set, fullscreen targets this node (e.g. a fixed `connected-terminal-container`); otherwise the terminal flex root inside this component. */
  fullscreenTargetRef?: React.RefObject<HTMLElement | null>;

  /** Auto-focus the terminal once it is ready. Defaults to "not on a mobile viewport", where focusing pops the soft keyboard. */
  autoFocus?: boolean;
  /** Prevent the terminal taking focus on pointer/touch. Defaults to "on mobile while the keyboard is closed". */
  preventFocusOnTap?: boolean;
  /** Show the tap-to-type keyboard affordance. Defaults to the mobile viewport, where the soft keyboard is the only way in. */
  showMobileKeyboard?: boolean;

  /** Shortcut presets. When non-empty, renders the `ShortcutDrawer` overlay (desktop and mobile). */
  mobileShortcuts?: ToolShortcutDef[];

  /** Called with a function that returns keyboard focus to this terminal. */
  onRegisterFocus?: (focus: () => void) => void;
  /** Called with a function that types text into the terminal input (no newline). Lets the session
   *  runtime expose this terminal's insert to the inspector's Files tab (click/tap route). */
  onRegisterInsertInput?: (insertInput: (text: string) => void) => void;

  /**
   * Fired once when the remote terminal session ends — the feed said so ({@link TerminalFeed.ended}),
   * i.e. process/session teardown rather than the user choosing Disconnect in the menu.
   */
  onRemoteSessionEnded?: () => void;

  /**
   * Fired per terminal I/O event: once per received output chunk (`bytesIn`) and once per chunk
   * sent (`bytesOut`). The sessions drawer folds these into the per-session runtime's counters so
   * the inspector's I/O byte meter ticks live, even for a backgrounded session.
   */
  onBytes?: (delta: ByteDelta) => void;

  /** Called per output frame with the cumulative byte offset the client has now received up to.
   *  On a replay / catch-up frame (`endOffset > 0`) the offset snaps to the frame's absolute
   *  `endOffset`; a live tail frame (`endOffset === 0`) that follows it advances the offset by its
   *  byte length, while frames that PRECEDE it on the same open (the re-issued mode prologue) are
   *  out-of-band VT state and leave it untouched — see `TerminalStreamOffset`. The parent uses this
   *  to track `currentOffset` so a reconnect can resume with `FROM_OFFSET` instead of re-replaying
   *  (no duplicates). */
  onOffsetUpdate?: (offset: bigint) => void;

  /** Expose the raw streamed text and byte count for E2E assertions (the `ghostty-*` suite). */
  showBufferTextForTest?: boolean;
  /** Skip Ghostty rendering and surface the received-chunk count/sample instead (E2E RPC probe). */
  debugMode?: boolean;
  /** Log data flow and lifecycle events to the console. */
  debugLogging?: boolean;
}

export function GhosttyTerminalSession({
  feed,
  sessionToken,
  sessionId,
  connectionStatus,
  connectionError,
  connectionOverlay,
  connectionChromePlacement = "floating",
  fontSize = 14,
  minFontSize = DEFAULT_TERMINAL_FONT_MIN,
  maxFontSize = DEFAULT_TERMINAL_FONT_MAX,
  fixedViewportGrid,
  terminalContainerMinHeightPx,
  fullscreenTargetRef: fullscreenTargetRefProp,
  autoFocus,
  preventFocusOnTap,
  showMobileKeyboard,
  mobileShortcuts,
  onRegisterFocus,
  onRegisterInsertInput,
  onRemoteSessionEnded,
  onBytes,
  onOffsetUpdate,
  showBufferTextForTest = false,
  debugMode = false,
  debugLogging = false,
}: GhosttyTerminalSessionProps) {
  const log = debugLogging
    ? (...args: unknown[]) => console.log("[GhosttyTerminalSession]", ...args)
    : () => {};

  const termRef = useRef<GhosttyTerminalHandle>(null);
  const olderTermRef = useRef<GhosttyTerminalHandle>(null);
  const liveContainerRef = useRef<HTMLDivElement>(null);
  const pageContainerRef = useRef<HTMLDivElement>(null);
  const internalFullscreenTargetRef = useRef<HTMLDivElement>(null);
  const fullscreenTargetRef = fullscreenTargetRefProp ?? internalFullscreenTargetRef;
  const olderReadyRef = useRef(false);
  const olderBufferRef = useRef<Uint8Array[]>([]);
  const termReadyRef = useRef(false);
  const outputBufferRef = useRef<Uint8Array[]>([]);
  const streamedTextRef = useRef("");

  const [bufferText, setBufferText] = useState("");
  const [olderBufferText, setOlderBufferText] = useState("");
  const [streamedByteCount, setStreamedByteCount] = useState(0);
  const [highlightedLine, setHighlightedLine] = useState("");
  const [firstOutputReceived, setFirstOutputReceived] = useState(false);
  const [rpcReceivedCount, setRpcReceivedCount] = useState(0);
  const [rpcReceivedSample, setRpcReceivedSample] = useState("");
  // Viewport position mirrors for the two panes (lines up from the bottom). Surfaced through hidden
  // elements so component tests can assert scrollToLine gives full control of the viewport.
  const [pageViewportY, setPageViewportY] = useState(0);
  const [liveViewportY, setLiveViewportY] = useState(0);
  const [liveScrollbackLength, setLiveScrollbackLength] = useState(0);
  const [pageScrollbar, setPageScrollbar] = useState("");
  const [liveScrollbar, setLiveScrollbar] = useState("");

  const isMobile = useIsMobile();
  const { isKeyboardOpen } = useVisualViewport();
  const keyboardAffordanceShown = showMobileKeyboard ?? isMobile;
  const focusesOnReady = autoFocus ?? !isMobile;
  const keepsFocusOffTap = preventFocusOnTap ?? (isMobile && !isKeyboardOpen);

  // The remote end is gone: the feed settled its `ended`. Input stops going out (there is nothing
  // reading it) and the pane says so, rather than looking interactive over a dead PTY.
  const [sessionEnded, setSessionEnded] = useState(false);
  const sessionEndedRef = useRef(false);

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

  // Whether this terminal can page back at all — the feed's answer, not the transport's. This is
  // the whole of what gives a LiveKit-carried session scrollback: the same question, asked of the
  // connection rather than of the component that happened to be rendering.
  const canScrollBack = feedSupportsHistory(feed);
  const historyRef = useRef(feed.history);
  historyRef.current = feed.history;

  // Cumulative output offset the client has received up to, carried across stream opens: each open
  // gets its own `TerminalStreamOffset` seeded from here (see the accounting rules there), and this
  // ref holds the latest value for the next open and for the history fill's upper bound. Surfaced to
  // the parent via `onOffsetUpdate` so a reconnect can resume with `FROM_OFFSET` instead of
  // re-replaying (no duplicates).
  const currentOffsetRef = useRef(0n);
  const onOffsetUpdateRef = useRef(onOffsetUpdate);
  onOffsetUpdateRef.current = onOffsetUpdate;
  const onBytesRef = useRef(onBytes);
  onBytesRef.current = onBytes;
  const onRemoteSessionEndedRef = useRef(onRemoteSessionEnded);
  onRemoteSessionEndedRef.current = onRemoteSessionEnded;

  const refreshMirrors = () => {
    setPageViewportY(olderTermRef.current?.getViewportScrollOffset?.() ?? 0);
    setLiveViewportY(termRef.current?.getViewportScrollOffset?.() ?? 0);
    setLiveScrollbackLength(termRef.current?.getScrollbackLength?.() ?? 0);
    const pageSb = olderTermRef.current?.getScrollbar?.();
    setPageScrollbar(pageSb ? `${pageSb.total},${pageSb.offset},${pageSb.len}` : "");
    const liveSb = termRef.current?.getScrollbar?.();
    setLiveScrollbar(liveSb ? `${liveSb.total},${liveSb.offset},${liveSb.len}` : "");
  };

  /**
   * The one path to the feed's input side — Ghostty's `onData`, the resize OSC and every affordance.
   *
   * `metered` is false for the terminal's own chatter. The resize OSC is not the operator's traffic:
   * the PTY bridge parses it out of the input stream and the shell never sees it, and a terminal
   * emits one the moment it is laid out. Counting it would have the inspector's I/O meter reading a
   * few dozen bytes for a session nobody has typed into, which is the opposite of what that number
   * is read for.
   */
  const writeToFeed = (data: string | Uint8Array, metered: boolean) => {
    // Nothing is reading the far end any more. Queueing bytes for a PTY that has exited is how a
    // terminal comes to look interactive over a session that ended.
    if (sessionEndedRef.current) return;
    const encoded = typeof data === "string" ? new TextEncoder().encode(data) : data;
    if (debugLogging) {
      console.log("[terminal→server]", describeKey(encoded), `(${encoded.length} bytes)`, Array.from(encoded));
    }
    if (metered) onBytesRef.current?.({ bytesOut: encoded.length });
    feed.stream.send(encoded);
  };

  /** Send what the operator produced — keystrokes, shortcuts, dropped paths. */
  const sendInput = (data: string | Uint8Array) => writeToFeed(data, true);

  // Expose text-insert and focus to the runtime (Files-tab click/tap route; focus-on-select). Refs
  // keep the registered functions current without re-registering every render.
  const sendInputRef = useRef(sendInput);
  sendInputRef.current = sendInput;
  useEffect(() => {
    onRegisterInsertInput?.((text: string) => sendInputRef.current(text));
  }, [onRegisterInsertInput]);
  useEffect(() => {
    onRegisterFocus?.(() => {
      termRef.current?.focus();
    });
  }, [onRegisterFocus]);

  useEffect(() => {
    // One accounting instance per feed, seeded with what the previous one had received: frames that
    // precede this open's offset-anchored frame (the re-issued mode prologue) are replayed VT state,
    // not stream bytes, and must not move the offset.
    const streamOffset = new TerminalStreamOffset(currentOffsetRef.current);
    let received = 0;
    feed.stream.onMessage((frame) => {
      const data = frame.data;
      received += 1;
      onBytesRef.current?.({ bytesIn: data.length });
      if (received === 1) setFirstOutputReceived(true);
      // Capture the lazy-history anchor from the initial replay frame (endOffset > 0). The anchor
      // drives the forward fill of the older-history terminal; storing it in state ensures the
      // affordance re-renders immediately when it is captured. A wire that carries no offsets never
      // produces one, and its fill runs unanchored — see `startForwardFill`.
      if (frame.endOffset > 0n && anchorRef.current === null) {
        const a = { endOffset: frame.endOffset, atOldest: frame.atOldest };
        anchorRef.current = a;
        setAnchor(a);
        dHistory(
          "lazy history anchor endOffset=%s atOldest=%o sessionId=%s",
          frame.endOffset.toString(),
          frame.atOldest,
          sessionId ?? "",
        );
      }
      // Advance the cumulative output offset and surface it to the parent so a reconnect resumes
      // with `FROM_OFFSET` (no duplicate replay).
      currentOffsetRef.current = streamOffset.accept(frame);
      onOffsetUpdateRef.current?.(currentOffsetRef.current);

      if (debugMode) {
        setRpcReceivedCount(received);
        setRpcReceivedSample(new TextDecoder().decode(data.slice(0, 200)));
        return;
      }
      if (showBufferTextForTest) {
        streamedTextRef.current += new TextDecoder().decode(data);
        setStreamedByteCount((count) => count + data.length);
      }
      const ready = termReadyRef.current && !!termRef.current;
      if (dTerm.enabled) {
        dTerm("recv %d bytes ready=%o %s", data.length, ready, hexPreview(data));
      }
      if (ready && termRef.current) {
        termRef.current.write(data);
      } else {
        // The reconnect byte-buffering fix: bytes that arrive before ghostty-web is ready are held
        // and flushed in order on `onReady`, instead of being written into a terminal that is not
        // there (which is what garbled a 220-column replay).
        outputBufferRef.current.push(data);
      }
    });
    return () => {
      feed.stream.close();
    };
  }, [feed, debugMode, showBufferTextForTest]);

  // The far end going away. `ended` never rejects and a feed that cannot tell simply never settles
  // it, so a terminal on such a wire keeps tailing rather than being told a lie.
  useEffect(() => {
    if (!feed.ended) return;
    let watching = true;
    void feed.ended.then(() => {
      if (!watching) return;
      sessionEndedRef.current = true;
      setSessionEnded(true);
      onRemoteSessionEndedRef.current?.();
    });
    return () => {
      watching = false;
    };
  }, [feed]);

  useEffect(() => {
    const interval = setInterval(() => {
      // Ghostty's parsed buffer is the clean text; the raw streamed ANSI is the fallback for the
      // E2E probe, whose assertion is "bytes arrived" and which runs before a frame is painted.
      const fromBuffer = termRef.current?.getBufferText?.() ?? "";
      setBufferText(showBufferTextForTest ? fromBuffer || streamedTextRef.current : fromBuffer);
      setOlderBufferText(olderTermRef.current?.getBufferText?.() ?? "");
      if (showBufferTextForTest) {
        const lines = termRef.current?.getBufferLines?.() ?? [];
        const inverseLine = lines.find((l) => l.hasInverse && l.text.trim().length > 0);
        if (inverseLine) setHighlightedLine(inverseLine.text);
      }
      refreshMirrors();
    }, 200);
    return () => clearInterval(interval);
  }, [showBufferTextForTest]);

  // Drive the progressive forward fill of the older-history page terminal in the background: append
  // one forward chunk at a time (oldest→anchor) until the loader reports done. While filling, the
  // loading indicator is shown over the foreground terminal; on completion the page terminal swaps
  // to the foreground (landed at its bottom = the newest pre-anchor line, seamless). Live bytes keep
  // flowing to the live terminal independently — no buffering, no reset.
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
    // `null` where no anchor was ever captured, which is a wire whose frames carry no offsets at
    // all (`terminal.TerminalOutput` is `bytes data` and nothing else). There the fill runs to the
    // capture tip and ends on the chunk's own `atEnd` — see `TerminalHistoryForwardLoader`.
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
    if (!el || !canScrollBack) return;
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
  }, [canScrollBack, filled]);

  // Scroll-down-at-bottom gesture on the PAGE terminal: when the user scrolls down while pinned to
  // the bottom of the older-history page, swap back to the live terminal (seamless return to the
  // live tip). Capture phase for the same reason as the live-pane listener.
  useEffect(() => {
    const el = pageContainerRef.current;
    if (!el || !canScrollBack) return;
    const handler = (e: WheelEvent) => {
      if (e.deltaY <= 0) return;
      if (olderTermRef.current?.isPinnedToBottom?.() ?? false) {
        setView("live");
      }
    };
    el.addEventListener("wheel", handler, { capture: true });
    return () => el.removeEventListener("wheel", handler, { capture: true } as AddEventListenerOptions);
  }, [canScrollBack]);

  // The "Load earlier output" affordance is shown on the live pane before the first fill (older
  // history can be served and is not yet loaded). After the first fill, the live pane shows a
  // "View history" affordance instead (instant swap, no fetch). The anchor only ever *withdraws*
  // the offer — a wire that states `atOldest` has nothing older — because a wire that states no
  // anchor at all is not a wire without history; it is one whose history the connection fetches
  // from somewhere else.
  const loadEarlierVisible = canScrollBack && !anchor?.atOldest && !filled && !loading;
  const viewHistoryVisible = canScrollBack && filled && view === "live";
  const backToLiveVisible = view === "page";

  // The status the chrome reports. A caller that states none is not making a claim about the
  // connection — the feed exists, so bytes can flow — and the raw readout stays out of the layout.
  const status: LiveKitChromeStatus = connectionStatus ?? "connected";
  const showStatusStrip =
    connectionStatus !== undefined &&
    shouldShowVisibleLiveKitStatusStrip({
      connectionOverlayEnabled: !!connectionOverlay,
      status,
    });
  const statusBarCompact = connectionChromePlacement === "none";

  // When both are present the terminal can upload; narrowing here removes the need for non-null
  // assertions at every use site (the props are optional for the non-session reuse).
  const uploadTarget = sessionToken && sessionId ? { sessionToken, sessionId } : null;

  const mobileKeyboardAffordance = keyboardAffordanceShown ? (
    <span className="inline-flex items-center gap-2">
      <MobileTerminalKeyboard onSend={sendInput} />
      {uploadTarget && (
        <TerminalUploadButton
          sessionToken={uploadTarget.sessionToken}
          sessionId={uploadTarget.sessionId}
          insertInput={sendInput}
        />
      )}
    </span>
  ) : null;

  tddyDevDebug("[GhosttyTerminalSession] render chrome", {
    hasConnectionOverlay: !!connectionOverlay,
    statusBarCompact,
    keyboardAffordanceShown,
    status,
  });

  const terminal = (
    <GhosttyTerminal
      ref={termRef}
      fontSize={fontSize}
      minFontSize={minFontSize}
      maxFontSize={maxFontSize}
      containerMinHeightPx={terminalContainerMinHeightPx}
      fixedViewportGrid={fixedViewportGrid}
      sessionActive={!sessionEnded}
      debugLogging={debugLogging}
      preventFocusOnTap={keepsFocusOffTap}
      onReady={() => {
        termReadyRef.current = true;
        const buf = outputBufferRef.current;
        outputBufferRef.current = [];
        const term = termRef.current;
        if (term) {
          log("ready — flushing", buf.length, "buffered chunk(s)");
          dTerm("ready — flushing %d buffered chunk(s)", buf.length);
          for (const chunk of buf) {
            term.write(chunk);
          }
          if (focusesOnReady) {
            term.focus();
          }
        }
      }}
      onData={(data) => {
        sendInput(data);
      }}
      onResize={(size) => {
        dResize(
          "OSC resize send cols=%d rows=%d fixedGrid=%o",
          size.cols,
          size.rows,
          fixedViewportGrid ?? null,
        );
        writeToFeed(`\x1b]resize;${size.cols};${size.rows}\x07`, false);
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
  const paneCoverStyle: React.CSSProperties = {
    position: "absolute",
    inset: 0,
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    padding: 16,
    backgroundColor: "rgba(0,0,0,0.55)",
    color: "#e0e0e0",
    fontSize: 14,
    textAlign: "center",
    pointerEvents: "auto",
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
      {showStatusStrip ? (
        <div data-testid="livekit-status">{status}</div>
      ) : (
        <div data-testid="livekit-status" hidden aria-hidden="true">
          {status}
        </div>
      )}
      {connectionOverlay ? (
        <TerminalConnectionStatusBar className="border-b border-border bg-muted">
          <ConnectionTerminalChrome
            chromeLayout="statusBar"
            overlayStatus={status}
            buildId={connectionOverlay.buildId}
            onDisconnect={connectionOverlay.onDisconnect}
            onTerminate={connectionOverlay.onTerminate}
            fullscreenTargetRef={fullscreenTargetRef}
            statusBarShowFullscreen={!statusBarCompact}
            statusBarShowBuildId={!statusBarCompact}
            statusBarEndSlot={mobileKeyboardAffordance}
          />
        </TerminalConnectionStatusBar>
      ) : null}
      <div
        ref={internalFullscreenTargetRef}
        style={{ flex: 1, minHeight: 0, minWidth: 0, width: "100%", position: "relative" }}
      >
        {debugMode ? (
          <div data-testid="rpc-debug-panel">
            <div data-testid="rpc-received-count">{rpcReceivedCount}</div>
            <div
              data-testid="rpc-received-sample"
              style={{ fontSize: 10, fontFamily: "monospace", wordBreak: "break-all" }}
            >
              {rpcReceivedSample || "(waiting…)"}
            </div>
          </div>
        ) : (
          <>
            {/* Live pane: scrollback 0 (always pinned to the live tip), always mounted, always
                receives the stream. Foreground at the tip; stays mounted underneath while browsing
                history. */}
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
              {uploadTarget ? (
                <TerminalFileDropZone
                  sessionToken={uploadTarget.sessionToken}
                  sessionId={uploadTarget.sessionId}
                  insertInput={sendInput}
                >
                  {terminal}
                </TerminalFileDropZone>
              ) : (
                terminal
              )}
              {mobileShortcuts && mobileShortcuts.length > 0 && (
                <ShortcutDrawer shortcuts={mobileShortcuts} onSend={sendInput} />
              )}
            </div>
            {/* Page pane: scrollback > 0, holds the forward-filled older history. Background (hidden)
                while being filled; swaps to the foreground once the fill completes. Hidden entirely
                (not unmounted) so its scrollback survives across swaps. */}
            {canScrollBack ? (
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
          </>
        )}
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
        {status === "error" && connectionError ? (
          <div data-testid="livekit-error" role="alert" style={{ ...paneCoverStyle, zIndex: 48 }}>
            {connectionError}
          </div>
        ) : null}
        {sessionEnded && (
          <div
            data-testid="terminal-coder-unavailable"
            role="status"
            style={{ ...paneCoverStyle, zIndex: 50 }}
          >
            Session ended — the coder disconnected. Reconnect from the session list to continue.
          </div>
        )}
      </div>
      {/* The keyboard affordance rides with the connection chrome where there is any — that bar is
          already above the canvas and holds the rest of the controls. Where there is none (the
          sessions screen, which shows connection state on its own overlay), it gets a strip of its
          own along the bottom, next to the thumb that reaches for it, rather than a bar of chrome
          the pane deliberately does without. */}
      {!connectionOverlay && mobileKeyboardAffordance ? (
        <div className="flex-shrink-0 flex items-center justify-center gap-2 border-t border-border bg-muted p-1">
          {mobileKeyboardAffordance}
        </div>
      ) : null}
      {firstOutputReceived && (
        <div data-testid="first-output-received" style={{ display: "none" }} aria-hidden />
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
      {showBufferTextForTest && (
        <>
          <div data-testid="streamed-byte-count" style={{ display: "none" }} aria-hidden>
            {streamedByteCount}
          </div>
          <div data-testid="terminal-highlighted-line" style={{ display: "none" }} aria-hidden>
            {highlightedLine}
          </div>
        </>
      )}
    </div>
  );
}
