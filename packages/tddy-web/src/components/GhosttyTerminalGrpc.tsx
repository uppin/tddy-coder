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

/** Scrollback lines retained by the older-history terminal (large enough to hold a full session). */
const OLDER_SCROLLBACK = 50000;

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
  /** Forward-fill fetcher for the older-history terminal. When provided, a "Load earlier output"
   *  affordance (and a scroll-up-on-live gesture) reveals a second, read-only ghostty-web terminal
   *  above the live one and progressively appends older output forward from offset 0 toward the
   *  anchor `endOffset`. No resets; the live terminal stays at `scrollback: 0`. */
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
  const olderReadyRef = useRef(false);
  const olderBufferRef = useRef<Uint8Array[]>([]);
  const termReadyRef = useRef(false);
  const outputBufferRef = useRef<Uint8Array[]>([]);
  const [bufferText, setBufferText] = useState("");
  const [olderBufferText, setOlderBufferText] = useState("");
  const isMobile = useIsMobile();
  const { isKeyboardOpen } = useVisualViewport();

  // Lazy scroll-up forward-fill state. The anchor (`endOffset`/`atOldest`) is captured from the
  // initial `StreamTerminalOutput` replay frame; the loader drives the progressive forward fill of
  // the older-history terminal. No resets — the live terminal just keeps appending live bytes.
  const loaderRef = useRef<TerminalHistoryForwardLoader | null>(null);
  const anchorRef = useRef<{ endOffset: bigint; atOldest: boolean } | null>(null);
  const [anchor, setAnchor] = useState<{ endOffset: bigint; atOldest: boolean } | null>(null);
  const fillingRef = useRef(false);
  const [olderVisible, setOlderVisible] = useState(false);
  const [filling, setFilling] = useState(false);
  const [fillDone, setFillDone] = useState(false);
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
    }, 200);
    return () => clearInterval(interval);
  }, []);

  // Drive the progressive forward fill of the older-history terminal: append one forward chunk at
  // a time (oldest→anchor) until the loader reports done. Live bytes keep flowing to the live
  // terminal independently — no buffering, no reset.
  const startForwardFill = async () => {
    if (fillingRef.current || fillDone) return;
    const fetcher = historyFetcherRef.current;
    const anchor = anchorRef.current;
    if (!fetcher || !anchor || anchor.atOldest || anchor.endOffset <= 0n) return;
    fillingRef.current = true;
    setFilling(true);
    setOlderVisible(true);
    if (loaderRef.current === null) {
      loaderRef.current = new TerminalHistoryForwardLoader(anchor.endOffset, anchor.atOldest);
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
      setFilling(false);
      setFillDone(loader.done);
    }
  };

  // Reveal the older terminal and begin the fill when the user activates the affordance.
  const onActivateLoadEarlier = () => {
    void startForwardFill();
  };

  // Scroll-up-on-live gesture: the live terminal has scrollback 0 (always pinned to bottom), so a
  // wheel-up attempt while pinned is interpreted as "show older history" and starts the fill. The
  // listener is attached in the CAPTURE phase so it fires before ghostty-web's own wheel handler
  // (which may stop propagation); a React `onWheel` (bubble phase) would never see the event.
  useEffect(() => {
    const el = liveContainerRef.current;
    if (!el || !historyFetcher) return;
    const handler = (e: WheelEvent) => {
      if (e.deltaY >= 0) return;
      if (termRef.current?.isPinnedToBottom?.() ?? true) {
        void startForwardFill();
      }
    };
    el.addEventListener("wheel", handler, { capture: true });
    return () => el.removeEventListener("wheel", handler, { capture: true } as AddEventListenerOptions);
  }, [historyFetcher]);

  const affordanceVisible =
    !!historyFetcher && !!anchor && !anchor.atOldest && !fillDone && !filling;

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
      scrollback={OLDER_SCROLLBACK}
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
      {historyFetcher ? (
        <div
          data-testid="terminal-older-pane"
          style={{
            flex: olderVisible ? "1 1 0" : "0 0 0",
            minHeight: 0,
            minWidth: 0,
            height: olderVisible ? "auto" : 0,
            overflow: "hidden",
            borderBottom: olderVisible ? "1px solid var(--border, #333)" : "none",
          }}
        >
          {olderTerminal}
        </div>
      ) : null}
      <div
        ref={liveContainerRef}
        style={{ flex: 1, minHeight: 0, minWidth: 0, width: "100%", position: "relative" }}
      >
        {affordanceVisible ? (
          <button
            type="button"
            data-testid="load-earlier-history"
            onClick={onActivateLoadEarlier}
            style={{
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
            }}
          >
            Load earlier output
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
    </div>
  );
}
