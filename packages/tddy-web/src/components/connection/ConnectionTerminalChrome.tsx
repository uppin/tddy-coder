import React, { useEffect, useId, useRef, useState } from "react";
import {
  exitDocumentFullscreen,
  isTargetInActiveFullscreen,
  requestFullscreenForConnectedTerminal,
} from "../../lib/browserFullscreen";
import { confirmRemoteSessionTermination } from "../../lib/remoteTerminateConfirm";
import { dataConnectionStatusValue } from "./connectionChromeStatus";

const TERMINATE_CONFIRM_COPY =
  "This will stop the remote session and end the process on the host. Continue?";

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

export type ConnectionTerminalChromeLayout = "corner" | "paneHeader";

export interface ConnectionTerminalChromeProps {
  overlayStatus: "connecting" | "connected" | "error";
  buildId?: string;
  onDisconnect: () => void;
  /** When set, Terminate entry is shown (SIGTERM path). */
  onTerminate?: () => void;
  /** Element to pass to the Fullscreen API (connected terminal subtree). */
  fullscreenTargetRef?: React.RefObject<HTMLElement | null>;
  /**
   * `corner` — status dot + fullscreen over the terminal canvas (default).
   * `paneHeader` — compact inline dot + menu for floating pane toolbars (no fullscreen/build id here).
   */
  chromeLayout?: ConnectionTerminalChromeLayout;
}

/**
 * Top-right status dot (with menu: Disconnect / optional Terminate). Interrupt (Stop) is the TUI Stop pane.
 */
export function ConnectionTerminalChrome({
  overlayStatus,
  buildId,
  onDisconnect,
  onTerminate,
  fullscreenTargetRef,
  chromeLayout = "corner",
}: ConnectionTerminalChromeProps) {
  const menuDomId = useId();
  const [menuOpen, setMenuOpen] = useState(false);
  const [fullscreenActive, setFullscreenActive] = useState(false);
  const internalFullscreenTargetRef = useRef<HTMLDivElement>(null);
  const resolvedFullscreenTargetRef = fullscreenTargetRef ?? internalFullscreenTargetRef;
  const dotRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const sync = () => {
      const target = resolvedFullscreenTargetRef.current ?? null;
      const active = isTargetInActiveFullscreen(target);
      console.debug("[tddy][ConnectionTerminalChrome] fullscreenchange", { active, hasTarget: target !== null });
      setFullscreenActive(active);
    };
    sync();
    document.addEventListener("fullscreenchange", sync);
    document.addEventListener("webkitfullscreenchange", sync as EventListener);
    return () => {
      document.removeEventListener("fullscreenchange", sync);
      document.removeEventListener("webkitfullscreenchange", sync as EventListener);
    };
  }, [resolvedFullscreenTargetRef]);

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
    if (!onTerminate) return;
    console.debug("[tddy][ConnectionTerminalChrome] Terminate menu action");
    if (!confirmRemoteSessionTermination(TERMINATE_CONFIRM_COPY)) {
      console.info("[tddy][ConnectionTerminalChrome] Terminate cancelled by user");
      return;
    }
    console.info("[tddy][ConnectionTerminalChrome] Terminate confirmed, invoking onTerminate");
    onTerminate();
  };

  const handleFullscreenClick = (ev: React.MouseEvent) => {
    ev.preventDefault();
    ev.stopPropagation();
    const target = resolvedFullscreenTargetRef.current ?? null;
    console.info("[tddy][ConnectionTerminalChrome] fullscreen button", {
      fullscreenActive,
      hasTarget: target !== null,
    });
    if (fullscreenActive) {
      void exitDocumentFullscreen();
      return;
    }
    void requestFullscreenForConnectedTerminal(target);
  };

  const statusAttr = dataConnectionStatusValue(overlayStatus);

  const menuPanel = menuOpen ? (
    <div
      ref={menuRef}
      id={menuDomId}
      role="menu"
      aria-label="Connection actions"
      style={
        chromeLayout === "paneHeader"
          ? {
              position: "absolute",
              top: "100%",
              right: 0,
              marginTop: 4,
              zIndex: 200,
              backgroundColor: "rgba(0,0,0,0.92)",
              border: "1px solid #555",
              borderRadius: 4,
              minWidth: 180,
              padding: 4,
            }
          : {
              position: "absolute",
              top: 52,
              right: 8,
              zIndex: 101,
              backgroundColor: "rgba(0,0,0,0.92)",
              border: "1px solid #555",
              borderRadius: 4,
              minWidth: 180,
              padding: 4,
            }
      }
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
  ) : null;

  if (chromeLayout === "paneHeader") {
    return (
      <>
        <style>
          {`
          @keyframes tddy-dot-pulse {
            0%, 100% { opacity: 1; transform: scale(1); }
            50% { opacity: 0.55; transform: scale(0.92); }
          }
          .tddy-connection-dot-inner {
            width: 10px;
            height: 10px;
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
        <div className="relative inline-flex shrink-0 items-center">
          <button
            ref={dotRef}
            type="button"
            data-testid="connection-status-dot"
            data-connection-status={statusAttr}
            aria-label="Connection status"
            aria-expanded={menuOpen}
            aria-haspopup="menu"
            aria-controls={menuDomId}
            style={{
              position: "relative",
              padding: "2px 6px",
              minWidth: 28,
              minHeight: 28,
              fontSize: 12,
              cursor: "pointer",
              backgroundColor: "rgba(0,0,0,0.35)",
              color: "#ccc",
              border: "1px solid #555",
              borderRadius: 4,
              zIndex: 100,
              pointerEvents: "auto",
            }}
            onClick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              toggleMenu();
            }}
          >
            <span className={dotClassForStatus(overlayStatus)} aria-hidden />
          </button>
          {menuPanel}
        </div>
      </>
    );
  }

  return (
    <>
      {!fullscreenTargetRef ? (
        <div
          ref={internalFullscreenTargetRef}
          data-testid="connection-chrome-fullscreen-fallback-target"
          style={{
            position: "absolute",
            inset: 0,
            zIndex: 0,
            pointerEvents: "none",
          }}
          aria-hidden
        />
      ) : null}
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
        aria-controls={menuDomId}
        style={{ ...CHROME_BTN, top: 8, right: 60, left: "auto", minWidth: 44, minHeight: 44 }}
        onClick={(e) => {
          e.preventDefault();
          e.stopPropagation();
          toggleMenu();
        }}
      >
        <span className={dotClassForStatus(overlayStatus)} aria-hidden />
      </button>
      <button
        type="button"
        data-testid="terminal-fullscreen-button"
        aria-label={fullscreenActive ? "Exit fullscreen" : "Enter fullscreen"}
        style={{ ...CHROME_BTN, top: 8, right: 8, left: "auto", minWidth: 44, minHeight: 44 }}
        onClick={handleFullscreenClick}
      >
        <span aria-hidden>⛶</span>
      </button>
      {menuPanel}
    </>
  );
}
