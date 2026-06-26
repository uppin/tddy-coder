import React, { useEffect, useRef, useState } from "react";
import { DEFAULT_TERMINAL_FONT_MAX, DEFAULT_TERMINAL_FONT_MIN } from "../lib/terminalZoom";
import { GhosttyTerminal, type GhosttyTerminalHandle } from "./GhosttyTerminal";
import { ConnectionTerminalChrome } from "./connection/ConnectionTerminalChrome";
import { TerminalConnectionStatusBar } from "./connection/TerminalConnectionStatusBar";

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
}

export function GhosttyTerminalGrpc({
  stream,
  connectionOverlay,
  onDisconnect,
  fontSize = 14,
  minFontSize = DEFAULT_TERMINAL_FONT_MIN,
  maxFontSize = DEFAULT_TERMINAL_FONT_MAX,
}: GhosttyTerminalGrpcProps) {
  const termRef = useRef<GhosttyTerminalHandle>(null);
  const termReadyRef = useRef(false);
  const outputBufferRef = useRef<Uint8Array[]>([]);
  const [bufferText, setBufferText] = useState("");

  useEffect(() => {
    stream.onMessage((data) => {
      if (termReadyRef.current && termRef.current) {
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
      onReady={() => {
        termReadyRef.current = true;
        const buf = outputBufferRef.current;
        outputBufferRef.current = [];
        const term = termRef.current;
        if (term) {
          for (const chunk of buf) {
            term.write(chunk);
          }
          term.focus();
        }
      }}
      onData={(data) => {
        stream.send(new TextEncoder().encode(data));
      }}
      onResize={(size) => {
        const seq = `\x1b]resize;${size.cols};${size.rows}\x07`;
        stream.send(new TextEncoder().encode(seq));
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
      </div>
      <div data-testid="terminal-buffer-text" style={{ display: "none" }} aria-hidden>
        {bufferText}
      </div>
    </div>
  );
}
