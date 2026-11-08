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
 * Docs: `packages/tddy-terminal-web/docs/terminal-session.md`.
 * Feature: `docs/ft/web/web-terminal.md`, `docs/ft/web/terminal-replay-lazy-scroll.md`.
 */


import React, { useEffect, useRef, useState } from "react";
import { DEFAULT_TERMINAL_FONT_MAX, DEFAULT_TERMINAL_FONT_MIN } from "./lib/terminalZoom";
import {
  shouldShowVisibleTerminalStatusStrip,
  type TerminalConnectionStatus,
} from "./connection/terminalConnectionStatus";
import { GhosttyTerminal } from "./GhosttyTerminal";
import type { GhosttyTerminalHandle } from "./terminal/types";
import { ConnectionTerminalChrome } from "./connection/ConnectionTerminalChrome";
import { TerminalConnectionStatusBar } from "./connection/TerminalConnectionStatusBar";
import { MobileTerminalKeyboard } from "./connection/MobileTerminalKeyboard";
import { TerminalFileDropZone } from "./connection/TerminalFileDropZone";
import { TerminalUploadButton } from "./connection/TerminalUploadButton";
import { ShortcutDrawer } from "./connection/ShortcutDrawer";
import { useIsMobile, useVisualViewport } from "tddy-ui-web";
import { feedSupportsHistory } from "tddy-client-web/connections/terminal";
import { dChrome, dResize, dTerm, describeKey } from "./session/sessionDiagnostics";
import { affordanceBtnStyle, paneCoverStyle } from "./session/paneStyles";
import { PaneOverlays } from "./session/PaneOverlays";
import { TerminalProbes } from "./session/TerminalProbes";
import type { GhosttyTerminalSessionProps } from "./session/types";
import { useFeedStream, useSessionEnded } from "./session/useFeedStream";
import { useHistoryDoubleBuffer } from "./session/useHistoryDoubleBuffer";
import { useTestWindowHooks } from "./session/useLivePaneGates";
import { useTerminalMirrors } from "./session/useTerminalMirrors";

export { describeKey } from "./session/sessionDiagnostics";
export type { GhosttyTerminalSessionProps } from "./session/types";

/**
 * The history view's greyish color scheme (also a stopped session's Agent tab), so it never reads as
 * the live terminal. The theme greys the page terminal's default background and foreground; the filter
 * desaturates the replayed output's own colors (SGR palette and truecolor alike), which a theme can't.
 */
export const HISTORY_THEME = { background: "#232326", foreground: "#9a9aa0" };
export const HISTORY_FILTER = "grayscale(1) brightness(0.9)";

/** Scrollback lines retained by the older-history page terminal (large enough to hold a full session). */
const PAGE_SCROLLBACK = 50000;

export function GhosttyTerminalSession({
  feed,
  sessionToken,
  sessionId,
  readsLocalPaths = false,
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
  const [streamedByteCount, setStreamedByteCount] = useState(0);

  const isMobile = useIsMobile();
  const { isKeyboardOpen } = useVisualViewport();
  const keyboardAffordanceShown = showMobileKeyboard ?? isMobile;
  const focusesOnReady = autoFocus ?? !isMobile;
  const keepsFocusOffTap = preventFocusOnTap ?? (isMobile && !isKeyboardOpen);

  // Cumulative output offset the client has received up to, carried across stream opens: each open
  // gets its own `TerminalStreamOffset` seeded from here (see the accounting rules there), and this
  // ref holds the latest value for the next open and for the history fill's upper bound. Surfaced to
  // the parent via `onOffsetUpdate`.
  const currentOffsetRef = useRef(0n);
  const onBytesRef = useRef(onBytes);
  onBytesRef.current = onBytes;

  const { mirrors, refreshMirrors } = useTerminalMirrors({
    termRef,
    olderTermRef,
    streamedTextRef,
    showBufferTextForTest,
  });

  const {
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
  } = useHistoryDoubleBuffer({
    feed,
    olderTermRef,
    olderReadyRef,
    olderBufferRef,
    currentOffsetRef,
    refreshMirrors,
  });

  const { sessionEnded, sessionEndedRef } = useSessionEnded(feed, onRemoteSessionEnded);

  const { firstOutputReceived, rpcReceivedCount, rpcReceivedSample } = useFeedStream({
    feed,
    sessionId,
    debugMode,
    showBufferTextForTest,
    onOffsetUpdate,
    onBytesRef,
    termRef,
    termReadyRef,
    outputBufferRef,
    streamedTextRef,
    setStreamedByteCount,
    currentOffsetRef,
    anchorRef,
    setAnchor,
    noteLiveModeSwitches,
  });

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

  /** Send what the operator produced — keystrokes, shortcuts. */
  const sendInput = (data: string | Uint8Array) => writeToFeed(data, true);

  /** Deliver dropped paths the way a native terminal does: as a paste, which ghostty wraps in
   *  bracketed-paste markers when the program enabled them (Claude Code does, and attaches a pasted
   *  image path as an image) and which reaches {@link sendInput} through `onData`. */
  const pasteInput = (text: string) => termRef.current?.paste?.(text);

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

  // FIXME: test-only hooks; remove if real wheel/keystroke drivers become available in Cypress.
  useTestWindowHooks(termRef, olderTermRef);

  // The status the chrome reports. A caller that states none is not making a claim about the
  // connection — the feed exists, so bytes can flow — and the raw readout stays out of the layout.
  const status: TerminalConnectionStatus = connectionStatus ?? "connected";
  const showStatusStrip =
    connectionStatus !== undefined &&
    shouldShowVisibleTerminalStatusStrip({
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

  dChrome("render chrome %o", {
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
            noteLiveModeSwitches(term, chunk);
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
      theme={HISTORY_THEME}
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
                  insertInput={pasteInput}
                  readsLocalPaths={readsLocalPaths}
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
                data-color-scheme="history"
                style={{
                  position: "absolute",
                  inset: 0,
                  zIndex: liveForeground ? 1 : 2,
                  visibility: liveForeground ? "hidden" : "visible",
                  pointerEvents: liveForeground ? "none" : "auto",
                  filter: HISTORY_FILTER,
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
        <PaneOverlays
          loading={loading}
          showError={status === "error" && !!connectionError}
          error={connectionError}
          sessionEnded={sessionEnded}
        />
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
      <TerminalProbes
        mirrors={mirrors}
        firstOutputReceived={firstOutputReceived}
        streamedByteCount={streamedByteCount}
        showBufferTextForTest={showBufferTextForTest}
      />
    </div>
  );
}
