import React, { useEffect, useRef, useState } from "react";
import { DisconnectReason, Room, RoomEvent } from "livekit-client";
import { createClient } from "@connectrpc/connect";
import {
  createLiveKitTransport,
  TerminalService,
  TerminalInputSchema,
} from "tddy-livekit-web";
import { create } from "@bufbuild/protobuf";
import { isCancelledLiveKitConnectionError } from "../lib/liveKitConnectionErrors";
import { tddyDevDebug } from "../lib/tddyDevLog";
import { shouldShowVisibleLiveKitStatusStrip } from "../lib/liveKitStatusPresentation";
import { DEFAULT_TERMINAL_FONT_MAX, DEFAULT_TERMINAL_FONT_MIN } from "../lib/terminalZoom";
import { GhosttyTerminal, type GhosttyTerminalHandle } from "./GhosttyTerminal";
import { ConnectionTerminalChrome } from "./connection/ConnectionTerminalChrome";
import { TerminalConnectionStatusBar } from "./connection/TerminalConnectionStatusBar";

/** Human-readable description of a terminal input byte sequence. */
function describeKey(bytes: Uint8Array): string {
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

export interface TokenResult {
  token: string;
  ttlSeconds: bigint;
}

export interface GhosttyTerminalLiveKitProps {
  url: string;
  /** Pre-generated JWT token. When both token and getToken provided, used for initial connect; getToken used for refresh. */
  token?: string;
  /** Callback to fetch/refresh token from Connect-RPC. When provided with token, used for refresh only; otherwise for initial + refresh. */
  getToken?: () => Promise<TokenResult>;
  /** TTL in seconds for refresh schedule. Required when both token and getToken provided. */
  ttlSeconds?: bigint;
  roomName?: string;
  showBufferTextForTest?: boolean;
  /** When true, skip Ghostty rendering and log RPC received data to console; expose count for test assertion. */
  debugMode?: boolean;
  /** When true, log data flows and lifecycle events to console for debugging. */
  debugLogging?: boolean;
  /** Called with a function to focus the terminal. Used by mobile keyboard button. */
  onRegisterFocus?: (focus: () => void) => void;
  /** When set, show connection chrome (status dot menu). Interrupt is the TUI Stop pane (SGR mouse → 0x03). */
  connectionOverlay?: {
    onDisconnect: () => void;
    buildId?: string;
    /** When set, dot menu includes Terminate (daemon session SIGTERM path). */
    onTerminate?: () => void;
  };
  /** When false, do not auto-focus terminal on ready (e.g. for mobile to avoid opening keyboard). Default true. */
  autoFocus?: boolean;
  /** When true, prevent terminal from receiving focus on pointer/touch (e.g. mobile when keyboard closed). */
  preventFocusOnTap?: boolean;
  /** When true, show mobile keyboard overlay (tap-to-type input). Must stay true while keyboard is open so input stays mounted. */
  showMobileKeyboard?: boolean;
  /** LiveKit identity of the server participant to target for RPC. Required when connecting to daemon sessions. */
  serverIdentity?: string;
  /** When set, fullscreen targets this node (e.g. fixed `connected-terminal-container`); otherwise the terminal flex root inside this component. */
  fullscreenTargetRef?: React.RefObject<HTMLElement | null>;
  /** Initial terminal font size (session baseline for Ctrl/⌘+0 reset). Default 14. */
  fontSize?: number;
  /** Bounds for zoom / pinch; default min 8, max 32. Overlay may pass a lower min (e.g. 2px). */
  minFontSize?: number;
  maxFontSize?: number;
  /**
   * Maps to `ConnectionTerminalChrome` `chromeLayout="statusBar"` inside `GhosttyTerminalLiveKit`:
   *
   * | `connectionChromePlacement` | Effect on status bar |
   * |------------------------------|----------------------|
   * | `"floating"` (default)       | Full bar: build id, dot, fullscreen, optional mobile keyboard slot |
   * | `"none"`                     | Compact bar: dot + menu (+ mobile keyboard); no build id / fullscreen (overlay panes) |
   */
  connectionChromePlacement?: "floating" | "none";
  /** Fired when LiveKit connection status changes (for header chrome when floating chrome is suppressed). */
  onConnectionStatusChange?: (status: "connecting" | "connected" | "error") => void;
  /** Passed to GhosttyTerminal — use `0` for fixed-height overlay panes (default 200px min is too tall). */
  terminalContainerMinHeightPx?: number;
  /** Fixed logical grid; font scales with container (floating overlay). */
  fixedViewportGrid?: { cols: number; rows: number };
  /**
   * Fired once when the remote terminal session ends: LiveKit **server participant** disconnects
   * (daemon/coder left the room) or the **terminal RPC stream** completes — i.e. underlying process
   * / session teardown, not the user choosing Disconnect in the menu.
   */
  onRemoteSessionEnded?: () => void;
}

export function GhosttyTerminalLiveKit({
  url,
  token,
  getToken,
  ttlSeconds: ttlSecondsProp,
  roomName = "terminal-e2e",
  showBufferTextForTest = false,
  debugMode = false,
  debugLogging = false,
  onRegisterFocus,
  autoFocus = true,
  preventFocusOnTap = false,
  showMobileKeyboard = false,
  serverIdentity = "server",
  connectionOverlay,
  fullscreenTargetRef: fullscreenTargetRefProp,
  fontSize = 14,
  minFontSize = DEFAULT_TERMINAL_FONT_MIN,
  maxFontSize = DEFAULT_TERMINAL_FONT_MAX,
  connectionChromePlacement = "floating",
  onConnectionStatusChange,
  terminalContainerMinHeightPx,
  fixedViewportGrid,
  onRemoteSessionEnded,
}: GhosttyTerminalLiveKitProps) {
  const log = debugLogging
    ? (...args: unknown[]) => console.log("[GhosttyLiveKit]", ...args)
    : () => {};
  const termRef = useRef<GhosttyTerminalHandle>(null);
  const internalFullscreenTargetRef = useRef<HTMLDivElement>(null);
  const fullscreenTargetRef = fullscreenTargetRefProp ?? internalFullscreenTargetRef;
  const inputQueueRef = useRef<Uint8Array[]>([]);
  const outputBufferRef = useRef<Uint8Array[]>([]);
  const streamedTextRef = useRef("");
  const termReadyRef = useRef(false);
  const latestTokenRef = useRef<string | null>(null);
  const refreshTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [status, setStatus] = useState<"connecting" | "connected" | "error">(
    "connecting"
  );
  const [bufferText, setBufferText] = useState("");
  const [streamedByteCount, setStreamedByteCount] = useState(0);
  const [errorMsg, setErrorMsg] = useState("");
  const [rpcReceivedCount, setRpcReceivedCount] = useState(0);
  const [rpcReceivedSample, setRpcReceivedSample] = useState("");
  const [firstOutputReceived, setFirstOutputReceived] = useState(false);
  const [highlightedLine, setHighlightedLine] = useState("");
  const [coderSessionActive, setCoderSessionActive] = useState(true);
  const coderAvailableRef = useRef(true);
  const onRemoteSessionEndedRef = useRef(onRemoteSessionEnded);
  const remoteSessionEndedEmittedRef = useRef(false);

  useEffect(() => {
    onRemoteSessionEndedRef.current = onRemoteSessionEnded;
  }, [onRemoteSessionEnded]);

  useEffect(() => {
    onConnectionStatusChange?.(status);
  }, [status, onConnectionStatusChange]);

  useEffect(() => {
    let room: Room | null = null;
    let bufferInterval: ReturnType<typeof setInterval> | null = null;
    let cancelled = false;

    const run = async () => {
      remoteSessionEndedEmittedRef.current = false;

      const emitRemoteSessionEnded = () => {
        if (cancelled) return;
        if (remoteSessionEndedEmittedRef.current) return;
        remoteSessionEndedEmittedRef.current = true;
        onRemoteSessionEndedRef.current?.();
      };

      try {
        coderAvailableRef.current = true;
        setCoderSessionActive(true);

        let initialToken: string;
        let ttlSecondsForRefresh: bigint | null = null;
        if (token && getToken && ttlSecondsProp !== undefined) {
          initialToken = token;
          latestTokenRef.current = token;
          ttlSecondsForRefresh = ttlSecondsProp;
        } else if (getToken) {
          const result = await getToken();
          initialToken = result.token;
          latestTokenRef.current = result.token;
          ttlSecondsForRefresh = result.ttlSeconds;
        } else if (token) {
          initialToken = token;
        } else {
          throw new Error("Either token or getToken must be provided");
        }

        if (getToken && ttlSecondsForRefresh !== null) {
          const ttlMs = Number(ttlSecondsForRefresh) * 1000;
          const refreshInMs = Math.max(0, ttlMs - 60 * 1000);
          const scheduleRefresh = (delayMs: number) => {
            refreshTimerRef.current = setTimeout(async () => {
              try {
                const next = await getToken();
                latestTokenRef.current = next.token;
                const nextTtlMs = Number(next.ttlSeconds) * 1000;
                const nextRefreshMs = Math.max(0, nextTtlMs - 60 * 1000);
                scheduleRefresh(nextRefreshMs);
              } catch (e) {
                console.warn("[GhosttyTerminalLiveKit] Token refresh failed:", e);
              }
            }, delayMs);
          };
          scheduleRefresh(refreshInMs);
        }

        room = new Room();

        room.on(RoomEvent.Connected, () => {
          log("lifecycle: RoomEvent.Connected");
          log("livekit: RoomEvent.Connected");
        });
        room.on(RoomEvent.Disconnected, async (reason) => {
          log("lifecycle: RoomEvent.Disconnected", reason);
          log("livekit: RoomEvent.Disconnected", reason);
          if (cancelled) return;
          if (getToken && latestTokenRef.current) {
            try {
              await room!.connect(url, latestTokenRef.current);
            } catch (e) {
              console.warn("[GhosttyTerminalLiveKit] Reconnect failed:", e);
            }
            return;
          }
          const r = reason ?? DisconnectReason.UNKNOWN_REASON;
          if (r === DisconnectReason.CLIENT_INITIATED) {
            return;
          }
          const msg =
            r === DisconnectReason.DUPLICATE_IDENTITY
              ? "Disconnected: another client joined with the same identity."
              : `LiveKit disconnected (reason=${r}).`;
          setErrorMsg(msg);
          setStatus("error");
        });
        room.on(RoomEvent.ConnectionStateChanged, (state) =>
          log("livekit: ConnectionStateChanged", state)
        );
        room.on(RoomEvent.ParticipantConnected, (participant) =>
          log("livekit: ParticipantConnected", participant.identity)
        );
        room.on(RoomEvent.ParticipantDisconnected, (participant) => {
          log("livekit: ParticipantDisconnected", participant.identity);
          if (participant.identity !== serverIdentity) return;
          if (cancelled) return;
          log(
            "livekit: server participant disconnected — terminal session marked inactive",
            { serverIdentity, participantIdentity: participant.identity },
          );
          coderAvailableRef.current = false;
          setCoderSessionActive(false);
          emitRemoteSessionEnded();
        });

        log("lifecycle: connecting to room");
        await room.connect(url, initialToken);
        log("lifecycle: room connected");

        const participantIdentities = () =>
          Array.from(room!.remoteParticipants.values()).map((p) => p.identity);
        const hasServer = () =>
          Array.from(room!.remoteParticipants.values()).some(
            (p) => p.identity === serverIdentity
          );

        log("livekit: Waiting for server participant", {
          lookingFor: serverIdentity,
          currentParticipants: participantIdentities(),
        });

        await new Promise<void>((resolve) => {
          if (hasServer()) return resolve();
          const handler = (participant: { identity: string }) => {
            log("livekit: ParticipantConnected (wait loop)", participant.identity, {
              lookingFor: serverIdentity,
              currentParticipants: participantIdentities(),
            });
            if (hasServer()) {
              room!.off(RoomEvent.ParticipantConnected, handler);
              resolve();
            }
          };
          room!.on(RoomEvent.ParticipantConnected, handler);
        });
        log("lifecycle: server participant found");

        const transport = createLiveKitTransport({
          room: room!,
          targetIdentity: serverIdentity,
          debug: debugMode || debugLogging,
        });
        log("lifecycle: transport created");

        const client = createClient(TerminalService, transport);

        const queue = inputQueueRef.current;
        async function* inputGen() {
          log("dataflow: inputGen started, yielding empty init");
          yield create(TerminalInputSchema, { data: new Uint8Array(0) });
          while (true) {
            if (queue.length > 0) {
              // Drain all available items into one message to avoid flooding the LiveKit data
              // channel with hundreds of tiny packets (e.g. rapid typing or paste). The WebRTC
              // data channel send buffer overflows when >~300 small messages are queued at once,
              // causing silent drops. Batching reduces 1000 single-byte sends to a few large ones.
              const chunks = queue.splice(0);
              const totalLen = chunks.reduce((sum, c) => sum + c.length, 0);
              const data = new Uint8Array(totalLen);
              let offset = 0;
              for (const chunk of chunks) {
                data.set(chunk, offset);
                offset += chunk.length;
              }
              const preview = new TextDecoder().decode(data.slice(0, 40));
              log("dataflow: inputGen yielding", data.length, "bytes (batched from", chunks.length, "items)", JSON.stringify(preview));
              yield create(TerminalInputSchema, { data });
            } else {
              await new Promise((r) => setTimeout(r, 20));
            }
          }
        }

        log("lifecycle: starting streamTerminalIO");
        const stream = client.streamTerminalIO(inputGen());

        (async () => {
          let count = 0;
          try {
            let sample = "";
            for await (const output of stream) {
              if (output.data.length === 0) continue;
              count++;
              const preview = new TextDecoder().decode(output.data.slice(0, 60));
              if (debugMode) {
                console.log("[GhosttyTerminalLiveKit] RPC chunk #", count, "bytes:", output.data.length, "sample:", preview);
                sample = new TextDecoder().decode(output.data.slice(0, 200));
                setRpcReceivedCount(count);
                setRpcReceivedSample(sample);
              }
              if (!debugMode) {
                if (count === 1) {
                  setFirstOutputReceived(true);
                  log("dataflow: first RPC output received");
                }
                const decoded = new TextDecoder().decode(output.data);
                streamedTextRef.current += decoded;
                const ready = termReadyRef.current;
                const hasTerm = !!termRef.current;
                log("dataflow: RPC output chunk #", count, "bytes:", output.data.length, "termReady:", ready, "hasTerm:", hasTerm, "preview:", JSON.stringify(preview));
                if (coderAvailableRef.current && ready && termRef.current) {
                  log("dataflow: writing to term");
                  termRef.current.write(output.data);
                } else if (!coderAvailableRef.current) {
                  log("dataflow: skip write (coder session inactive)");
                } else {
                  log("dataflow: buffering (outputBuffer length:", outputBufferRef.current.length + 1, ")");
                  outputBufferRef.current.push(output.data);
                }
                if (showBufferTextForTest) {
                  setBufferText(streamedTextRef.current);
                  setStreamedByteCount((c) => c + output.data.length);
                }
              }
            }
            log("lifecycle: stream ended");
            console.warn("[GhosttyTerminalLiveKit] output stream ended after", count, "chunks — terminal will no longer receive updates");
            emitRemoteSessionEnded();
          } catch (e) {
            log("lifecycle: stream error", e);
            console.error("[GhosttyTerminalLiveKit] output stream error after", count, "chunks:", e);
          }
        })();

        setStatus("connected");
        log("lifecycle: status=connected");

        if (showBufferTextForTest && !debugMode) {
          bufferInterval = setInterval(() => {
            // Prefer Ghostty's parsed buffer (clean text) over raw streamed ANSI.
            const fromBuffer = termRef.current?.getBufferText?.() ?? "";
            const text = fromBuffer || streamedTextRef.current;
            setBufferText(text);

            const lines = termRef.current?.getBufferLines?.() ?? [];
            const inverseLine = lines.find((l) => l.hasInverse && l.text.trim().length > 0);
            if (inverseLine) {
              setHighlightedLine(inverseLine.text);
            }
          }, 200);
        }
      } catch (e) {
        if (cancelled) return;
        if (isCancelledLiveKitConnectionError(e)) return;
        const err = e instanceof Error ? e : new Error(String(e));
        setErrorMsg(err.message);
        setStatus("error");
      }
    };

    run();

    return () => {
      cancelled = true;
      if (refreshTimerRef.current) clearTimeout(refreshTimerRef.current);
      if (bufferInterval) clearInterval(bufferInterval);
      if (room) room.disconnect();
    };
  }, [url, token, getToken, ttlSecondsProp, roomName, serverIdentity, showBufferTextForTest, debugMode, debugLogging]);

  useEffect(() => {
    if (onRegisterFocus) {
      onRegisterFocus(() => {
        termRef.current?.focus();
      });
    }
  }, [onRegisterFocus]);

  /** Single path to the LiveKit input stream (same queue as Ghostty `onData`). */
  const enqueueTerminalInput = (encoded: Uint8Array) => {
    if (!coderAvailableRef.current) return;
    inputQueueRef.current.push(encoded);
    if (debugLogging) {
      const keyName = describeKey(encoded);
      console.log("[terminal→server]", keyName, `(${encoded.length} bytes)`, Array.from(encoded));
    }
  };

  const pushInput = (data: string | Uint8Array) => {
    const encoded =
      typeof data === "string" ? new TextEncoder().encode(data) : data;
    enqueueTerminalInput(encoded);
  };

  const handleMobileKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    // Ctrl+letter must send control bytes (e.g. Ctrl+C → 0x03). Otherwise `onInput` only sees "c"
    // (0x63) — the same bug as VirtualTui view_state swallowing Ctrl as text.
    if (e.ctrlKey && e.key.length === 1) {
      const lower = e.key.toLowerCase();
      if (lower >= "a" && lower <= "z") {
        e.preventDefault();
        pushInput(new Uint8Array([lower.charCodeAt(0) - 96]));
        return;
      }
    }
    const key = e.key;
    if (key === "Enter") {
      e.preventDefault();
      pushInput(new Uint8Array([0x0d]));
      return;
    }
    if (key === "Backspace") {
      e.preventDefault();
      pushInput(new Uint8Array([0x7f]));
      return;
    }
    if (key === "Tab") {
      e.preventDefault();
      pushInput(new Uint8Array([0x09]));
      return;
    }
    if (key === "Escape") {
      e.preventDefault();
      pushInput(new Uint8Array([0x1b]));
      return;
    }
    if (key.startsWith("Arrow")) {
      e.preventDefault();
      const seq =
        key === "ArrowUp"
          ? "\x1b[A"
          : key === "ArrowDown"
            ? "\x1b[B"
            : key === "ArrowRight"
              ? "\x1b[C"
              : key === "ArrowLeft"
                ? "\x1b[D"
                : null;
      if (seq) pushInput(seq);
    }
  };

  const handleMobileInput = (e: React.FormEvent<HTMLInputElement>) => {
    const input = e.currentTarget;
    const data = (e.nativeEvent as InputEvent).data;
    if (data) pushInput(data);
    input.value = "";
  };

  const showLiveKitStatusStrip = shouldShowVisibleLiveKitStatusStrip({
    connectionOverlayEnabled: !!connectionOverlay,
    status,
  });

  const statusBarCompact = connectionChromePlacement === "none";

  const mobileKeyboardAffordance = showMobileKeyboard ? (
    <label
      data-testid="mobile-keyboard-button"
      className="relative inline-flex shrink-0 cursor-pointer items-center rounded border border-input bg-background px-3 py-1 text-xs text-foreground"
    >
      <input
        type="text"
        autoComplete="off"
        autoCorrect="off"
        autoCapitalize="off"
        spellCheck={false}
        aria-label="Open keyboard for terminal input"
        className="absolute inset-0 h-full w-full opacity-0"
        style={{ margin: 0, border: "none", fontSize: 1 }}
        onKeyDown={handleMobileKeyDown}
        onInput={handleMobileInput}
      />
      <span className="pointer-events-none">Keyboard</span>
    </label>
  ) : null;

  tddyDevDebug("[GhosttyTerminalLiveKit] render chrome", {
    hasConnectionOverlay: !!connectionOverlay,
    statusBarCompact,
    showMobileKeyboard,
    status,
  });

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
      {connectionOverlay && !showLiveKitStatusStrip ? (
        <div data-testid="livekit-status" hidden aria-hidden="true">
          {status}
        </div>
      ) : (
        <div data-testid="livekit-status">{status}</div>
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
      ) : showMobileKeyboard ? (
        <TerminalConnectionStatusBar className="border-b border-border bg-muted">
          <div className="flex w-full justify-end px-2 py-1">{mobileKeyboardAffordance}</div>
        </TerminalConnectionStatusBar>
      ) : null}
      <div
        ref={internalFullscreenTargetRef}
        style={{ flex: 1, minHeight: 0, minWidth: 0, width: "100%", position: "relative" }}
      >
        {status === "error" && errorMsg ? (
          <div
            data-testid="livekit-error"
            role="alert"
            style={{
              position: "absolute",
              inset: 0,
              zIndex: 48,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              padding: 16,
              backgroundColor: "rgba(0,0,0,0.55)",
              color: "#e0e0e0",
              fontSize: 14,
              textAlign: "center",
              pointerEvents: "auto",
            }}
          >
            {errorMsg}
          </div>
        ) : null}
        {!coderSessionActive && (
          <div
            data-testid="terminal-coder-unavailable"
            role="status"
            style={{
              position: "absolute",
              inset: 0,
              zIndex: 50,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              padding: 16,
              backgroundColor: "rgba(0,0,0,0.55)",
              color: "#e0e0e0",
              fontSize: 14,
              textAlign: "center",
              pointerEvents: "auto",
            }}
          >
            Session ended — the coder disconnected. Reconnect from the session list to continue.
          </div>
        )}
        {debugMode ? (
          <div data-testid="rpc-debug-panel">
            <div data-testid="rpc-received-count">{rpcReceivedCount}</div>
            <div data-testid="rpc-received-sample" style={{ fontSize: 10, fontFamily: "monospace", wordBreak: "break-all" }}>
              {rpcReceivedSample || "(waiting…)"}
            </div>
          </div>
        ) : (
          <GhosttyTerminal
            ref={termRef}
            fontSize={fontSize}
            minFontSize={minFontSize}
            maxFontSize={maxFontSize}
            containerMinHeightPx={terminalContainerMinHeightPx}
            fixedViewportGrid={fixedViewportGrid}
            sessionActive={coderSessionActive}
            debugLogging={debugLogging}
            preventFocusOnTap={preventFocusOnTap}
            onReady={() => {
              termReadyRef.current = true;
              const buf = outputBufferRef.current;
              outputBufferRef.current = [];
              const term = termRef.current;
              log("lifecycle: onReady, flushing buffer", buf.length, "chunks");
              if (term) {
                for (const chunk of buf) {
                  log("dataflow: onReady flush write", chunk.length, "bytes");
                  term.write(chunk);
                }
                if (autoFocus !== false) {
                  term.focus();
                }
              }
            }}
            onResize={(size) => {
              const seq = `\x1b]resize;${size.cols};${size.rows}\x07`;
              enqueueTerminalInput(new TextEncoder().encode(seq));
            }}
            onData={(data) => {
              enqueueTerminalInput(new TextEncoder().encode(data));
            }}
          />
        )}
      </div>
      {firstOutputReceived && (
        <div data-testid="first-output-received" style={{ display: "none" }} aria-hidden />
      )}
      {showBufferTextForTest && (
        <>
          <div data-testid="streamed-byte-count" style={{ display: "none" }} aria-hidden>
            {streamedByteCount}
          </div>
          <div
            data-testid="terminal-buffer-text"
            style={{ display: "none" }}
            aria-hidden
          >
            {bufferText}
          </div>
          <div
            data-testid="terminal-highlighted-line"
            style={{ display: "none" }}
            aria-hidden
          >
            {highlightedLine}
          </div>
        </>
      )}
    </div>
  );
}
