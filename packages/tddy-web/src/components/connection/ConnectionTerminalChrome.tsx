import React, { useEffect, useRef, useState } from "react";
import { dataConnectionStatusValue } from "./connectionChromeStatus";

const CHROME_BTN: React.CSSProperties = {
  position: "absolute",
  padding: "4px 12px",
  fontSize: 12,
  cursor: "pointer",
  backgroundColor: "rgba(0,0,0,0.6)",
  color: "#ccc",
  border: "1px solid #555",
  borderRadius: 4,
  zIndex: 100,
  pointerEvents: "auto",
};

const MENU_BTN: React.CSSProperties = {
  display: "block",
  width: "100%",
  textAlign: "left",
  padding: "10px 14px",
  border: "none",
  background: "transparent",
  color: "#e5e5e5",
  cursor: "pointer",
  fontSize: 14,
};

function dotClassForStatus(overlayStatus: "connecting" | "connected" | "error"): string {
  if (overlayStatus === "connecting") return "tddy-connection-dot-inner tddy-connection-dot--connecting";
  if (overlayStatus === "connected") return "tddy-connection-dot-inner tddy-connection-dot--connected";
  return "tddy-connection-dot-inner tddy-connection-dot--error";
}

export interface ConnectionTerminalChromeProps {
  overlayStatus: "connecting" | "connected" | "error";
  buildId?: string;
  onDisconnect: () => void;
  /** When set, Terminate entry is shown (SIGTERM path). */
  onTerminate?: () => void;
  /** Same path as legacy Ctrl+C — parent binds enqueue of 0x03. */
  onStopInterrupt: () => void;
}

/**
 * Top-right status dot (with menu: Disconnect / optional Terminate) and bottom-right Stop.
 */
export function ConnectionTerminalChrome({
  overlayStatus,
  buildId,
  onDisconnect,
  onTerminate,
  onStopInterrupt,
}: ConnectionTerminalChromeProps) {
  const [menuOpen, setMenuOpen] = useState(false);
  const dotRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!menuOpen) return;
    const onPointerDown = (e: MouseEvent) => {
      const t = e.target as Node;
      if (dotRef.current?.contains(t)) return;
      if (menuRef.current?.contains(t)) return;
      setMenuOpen(false);
    };
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setMenuOpen(false);
      }
    };
    document.addEventListener("mousedown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("mousedown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [menuOpen]);

  const toggleMenu = () => {
    setMenuOpen((o) => !o);
  };

  const handleDisconnectClick = () => {
    setMenuOpen(false);
    onDisconnect();
  };

  const handleTerminateClick = () => {
    setMenuOpen(false);
    onTerminate?.();
  };

  const handleStopClick = (ev: React.MouseEvent) => {
    ev.preventDefault();
    ev.stopPropagation();
    onStopInterrupt();
  };

  const statusAttr = dataConnectionStatusValue(overlayStatus);

  return (
    <>
      <style>
        {`
          @keyframes tddy-dot-pulse {
            0%, 100% { opacity: 1; transform: scale(1); }
            50% { opacity: 0.55; transform: scale(0.92); }
          }
          .tddy-connection-dot-inner {
            width: 12px;
            height: 12px;
            border-radius: 50%;
            display: inline-block;
            vertical-align: middle;
          }
          .tddy-connection-dot--connecting {
            background: #f59e0b;
            animation: tddy-dot-pulse 1.2s ease-in-out infinite;
          }
          .tddy-connection-dot--connected {
            background: #22c55e;
          }
          .tddy-connection-dot--error {
            background: #ef4444;
          }
          @media (prefers-reduced-motion: reduce) {
            .tddy-connection-dot--connecting {
              animation: none;
              opacity: 0.85;
            }
          }
        `}
      </style>
      {buildId !== undefined && (
        <span
          data-testid="build-id"
          style={{
            ...CHROME_BTN,
            top: 8,
            left: 8,
            right: "auto",
            fontSize: 10,
            color: "#888",
            cursor: "default",
          }}
        >
          {buildId}
        </span>
      )}
      <button
        ref={dotRef}
        type="button"
        data-testid="connection-status-dot"
        data-connection-status={statusAttr}
        aria-label="Connection status"
        aria-expanded={menuOpen}
        aria-haspopup="menu"
        aria-controls="connection-terminal-chrome-menu"
        style={{ ...CHROME_BTN, top: 8, right: 8, left: "auto", minWidth: 44, minHeight: 44 }}
        onClick={(e) => {
          e.preventDefault();
          e.stopPropagation();
          toggleMenu();
        }}
      >
        <span className={dotClassForStatus(overlayStatus)} aria-hidden />
      </button>
      {menuOpen && (
        <div
          ref={menuRef}
          id="connection-terminal-chrome-menu"
          role="menu"
          aria-label="Connection actions"
          style={{
            position: "absolute",
            top: 52,
            right: 8,
            zIndex: 101,
            backgroundColor: "rgba(0,0,0,0.92)",
            border: "1px solid #555",
            borderRadius: 4,
            minWidth: 180,
            padding: 4,
          }}
        >
          <button
            type="button"
            role="menuitem"
            data-testid="connection-menu-disconnect"
            style={MENU_BTN}
            onClick={handleDisconnectClick}
          >
            Disconnect
          </button>
          {onTerminate ? (
            <button
              type="button"
              role="menuitem"
              data-testid="connection-menu-terminate"
              style={MENU_BTN}
              onClick={handleTerminateClick}
            >
              Terminate
            </button>
          ) : null}
        </div>
      )}
      <button
        type="button"
        data-testid="terminal-stop-button"
        aria-label="Send interrupt (Stop)"
        style={{ ...CHROME_BTN, bottom: 8, right: 8, top: "auto", left: "auto", minWidth: 44, minHeight: 44 }}
        onClick={handleStopClick}
      >
        Stop
      </button>
    </>
  );
}
