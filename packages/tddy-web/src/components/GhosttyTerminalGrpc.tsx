import React, { useEffect, useRef, useState } from "react";
import { DEFAULT_TERMINAL_FONT_MAX, DEFAULT_TERMINAL_FONT_MIN } from "../lib/terminalZoom";
import { tddyDebug } from "../lib/debugMask";
import { GhosttyTerminal, type GhosttyTerminalHandle } from "./GhosttyTerminal";
import { ConnectionTerminalChrome } from "./connection/ConnectionTerminalChrome";
import { TerminalConnectionStatusBar } from "./connection/TerminalConnectionStatusBar";
import { MobileTerminalKeyboard } from "./connection/MobileTerminalKeyboard";
import { ShortcutDrawer } from "./connection/ShortcutDrawer";
import { useIsMobile } from "../hooks/useIsMobile";
import { useVisualViewport } from "../hooks/useVisualViewport";
import type { ToolShortcutDef } from "../lib/toolShortcuts";

// `[tddy]` diagnostics for the gRPC terminal byte stream (enabled by the DEBUG mask).
// The 220-col garbling on reconnect lived here, so log incoming bytes / buffering / resize.
const dGrpc = tddyDebug("tddy:term:grpc");
const dResize = tddyDebug("tddy:term:resize");

/** Hex preview of the first `n` bytes for diagnosing garbled / misaligned output. */
function hexPreview(data: Uint8Array, n = 24): string {
  return Array.from(data.slice(0, n), (b) => b.toString(16).padStart(2, "0")).join(" ");
}

export interface GrpcStream {
  send(data: Uint8Array): void;
  onMessage(fn: (data: Uint8Array) => void): void;
  close(): void;
}

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
}

export function GhosttyTerminalGrpc({
  stream,
  connectionOverlay,
  onDisconnect,
  fontSize = 14,
  minFontSize = DEFAULT_TERMINAL_FONT_MIN,
  maxFontSize = DEFAULT_TERMINAL_FONT_MAX,
  mobileShortcuts,
}: GhosttyTerminalGrpcProps) {
  const termRef = useRef<GhosttyTerminalHandle>(null);
  const termReadyRef = useRef(false);
  const outputBufferRef = useRef<Uint8Array[]>([]);
  const [bufferText, setBufferText] = useState("");
  const isMobile = useIsMobile();
  const { isKeyboardOpen } = useVisualViewport();

  const sendInput = (data: string | Uint8Array) =>
    stream.send(typeof data === "string" ? new TextEncoder().encode(data) : data);

  useEffect(() => {
    stream.onMessage((data) => {
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
  }, [stream]);

  useEffect(() => {
    const interval = setInterval(() => {
      const text = termRef.current?.getBufferText?.() ?? "";
      setBufferText(text);
    }, 200);
    return () => clearInterval(interval);
  }, []);

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
        {terminal}
        {isMobile && mobileShortcuts && mobileShortcuts.length > 0 && (
          <ShortcutDrawer shortcuts={mobileShortcuts} onSend={sendInput} />
        )}
      </div>
      {isMobile && (
        <div className="flex-shrink-0 flex items-center justify-center border-t border-border bg-muted p-1">
          <MobileTerminalKeyboard onSend={sendInput} />
        </div>
      )}
      <div data-testid="terminal-buffer-text" style={{ display: "none" }} aria-hidden>
        {bufferText}
      </div>
    </div>
  );
}
