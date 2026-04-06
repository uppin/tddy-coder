import React, { useEffect, useId, useRef, useState } from "react";
import {
  exitDocumentFullscreen,
  isTargetInActiveFullscreen,
  requestFullscreenForConnectedTerminal,
} from "../../lib/browserFullscreen";
import { tddyDevDebug } from "../../lib/tddyDevLog";
import { confirmRemoteSessionTermination } from "../../lib/remoteTerminateConfirm";
import { dataConnectionStatusValue } from "./connectionChromeStatus";
import { CONNECTION_TERMINAL_DOT_STYLES } from "./connectionTerminalChromeDotStyles";

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

const MENU_DROPDOWN_BELOW: React.CSSProperties = {
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
};

const MENU_DROPDOWN_CORNER: React.CSSProperties = {
  position: "absolute",
  top: 52,
  right: 8,
  zIndex: 101,
  backgroundColor: "rgba(0,0,0,0.92)",
  border: "1px solid #555",
  borderRadius: 4,
  minWidth: 180,
  padding: 4,
};

function dotClassForStatus(
  overlayStatus: "connecting" | "connected" | "error",
  compact?: boolean,
): string {
  const state =
    overlayStatus === "connecting"
      ? "tddy-connection-dot--connecting"
      : overlayStatus === "connected"
        ? "tddy-connection-dot--connected"
        : "tddy-connection-dot--error";
  const inner = compact
    ? "tddy-connection-dot-inner tddy-connection-dot-inner--compact"
    : "tddy-connection-dot-inner";
  return `${inner} ${state}`;
}

export type ConnectionTerminalChromeLayout = "corner" | "paneHeader" | "statusBar";

export interface ConnectionTerminalChromeProps {
  overlayStatus: "connecting" | "connected" | "error";
  buildId?: string;
  onDisconnect: () => void;
  /** When set, Terminate entry is shown (SIGTERM path). */
  onTerminate?: () => void;
  /** Element to pass to the Fullscreen API (connected terminal subtree). */
  fullscreenTargetRef?: React.RefObject<HTMLElement | null>;
  /**
   * `corner` — status dot + fullscreen over the terminal canvas (legacy overlay).
   * `paneHeader` — compact inline dot + menu for floating pane toolbars (no fullscreen/build id here).
   * `statusBar` — horizontal toolbar row (PRD): build id, dot, fullscreen; no overlay on the grid.
   */
  chromeLayout?: ConnectionTerminalChromeLayout;
  /** When `chromeLayout` is `statusBar`, show fullscreen control. Default true. */
  statusBarShowFullscreen?: boolean;
  /** When `chromeLayout` is `statusBar`, show build id. Default true. */
  statusBarShowBuildId?: boolean;
  /** Trailing controls in the status bar row (e.g. mobile keyboard affordance). */
  statusBarEndSlot?: React.ReactNode;
}

/**
 * Connection status dot (menu: Disconnect / optional Terminate) and optional fullscreen.
 */
export function ConnectionTerminalChrome({
  overlayStatus,
  buildId,
  onDisconnect,
  onTerminate,
  fullscreenTargetRef,
  chromeLayout = "corner",
  statusBarShowFullscreen = true,
  statusBarShowBuildId = true,
  statusBarEndSlot,
}: ConnectionTerminalChromeProps) {
  const menuDomId = useId();
  const [menuOpen, setMenuOpen] = useState(false);
  const [fullscreenActive, setFullscreenActive] = useState(false);
  const internalFullscreenTargetRef = useRef<HTMLDivElement>(null);
  const resolvedFullscreenTargetRef = fullscreenTargetRef ?? internalFullscreenTargetRef;
  const dotRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    tddyDevDebug("[ConnectionTerminalChrome] mount", { chromeLayout, overlayStatus });
  }, [chromeLayout, overlayStatus]);

  useEffect(() => {
    const sync = () => {
      const target = resolvedFullscreenTargetRef.current ?? null;
      const active = isTargetInActiveFullscreen(target);
      tddyDevDebug("[ConnectionTerminalChrome] fullscreenchange", { active, hasTarget: target !== null });
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
    tddyDevDebug("[ConnectionTerminalChrome] Terminate menu action");
    if (!confirmRemoteSessionTermination(TERMINATE_CONFIRM_COPY)) {
      tddyDevDebug("[ConnectionTerminalChrome] Terminate cancelled by user");
      return;
    }
    tddyDevDebug("[ConnectionTerminalChrome] Terminate confirmed, invoking onTerminate");
    onTerminate();
  };

  const handleFullscreenClick = (ev: React.MouseEvent) => {
    ev.preventDefault();
    ev.stopPropagation();
    const target = resolvedFullscreenTargetRef.current ?? null;
    tddyDevDebug("[ConnectionTerminalChrome] fullscreen button", {
      fullscreenActive,
      hasTarget: target !== null,
      chromeLayout,
    });
    if (fullscreenActive) {
      void exitDocumentFullscreen();
      return;
    }
    void requestFullscreenForConnectedTerminal(target);
  };

  const statusAttr = dataConnectionStatusValue(overlayStatus);

  const menuDropdownStyle: React.CSSProperties =
    chromeLayout === "paneHeader" || chromeLayout === "statusBar" ? MENU_DROPDOWN_BELOW : MENU_DROPDOWN_CORNER;

  const menuPanel = menuOpen ? (
    <div
      ref={menuRef}
      id={menuDomId}
      role="menu"
      aria-label="Connection actions"
      style={menuDropdownStyle}
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

  if (chromeLayout === "statusBar") {
    const showBuild = buildId !== undefined && statusBarShowBuildId;
    const showFs = statusBarShowFullscreen;
    tddyDevDebug("[ConnectionTerminalChrome] render statusBar", {
      showBuild,
      showFs,
      hasEndSlot: !!statusBarEndSlot,
    });
    return (
      <>
        <style>{CONNECTION_TERMINAL_DOT_STYLES}</style>
        <div className="relative flex w-full min-w-0 items-center gap-2 px-2 py-1">
          <div className="flex min-w-0 flex-1 items-center gap-2 overflow-hidden">
            {showBuild ? (
              <span
                data-testid="build-id"
                className="truncate font-mono text-[10px]"
                style={{ color: "#888", cursor: "default" }}
              >
                {buildId}
              </span>
            ) : null}
          </div>
          <div className="relative flex shrink-0 items-center gap-1">
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
            <button
              ref={dotRef}
              type="button"
              data-testid="connection-status-dot"
              data-connection-status={statusAttr}
              aria-label="Connection status"
              aria-expanded={menuOpen}
              aria-haspopup="menu"
              aria-controls={menuDomId}
              className="relative shrink-0 cursor-pointer rounded border border-input bg-background/80 px-2 py-1"
              style={{
                minWidth: 36,
                minHeight: 32,
                fontSize: 12,
              }}
              onClick={(e) => {
                e.preventDefault();
                e.stopPropagation();
                toggleMenu();
              }}
            >
              <span className={dotClassForStatus(overlayStatus)} aria-hidden />
            </button>
            {showFs ? (
              <button
                type="button"
                data-testid="terminal-fullscreen-button"
                aria-label={fullscreenActive ? "Exit fullscreen" : "Enter fullscreen"}
                className="relative shrink-0 cursor-pointer rounded border border-input bg-background/80 px-2 py-1 text-foreground"
                style={{ minWidth: 36, minHeight: 32 }}
                onClick={handleFullscreenClick}
              >
                <span aria-hidden>⛶</span>
              </button>
            ) : null}
            {menuPanel}
          </div>
          {statusBarEndSlot}
        </div>
      </>
    );
  }

  if (chromeLayout === "paneHeader") {
    return (
      <>
        <style>{CONNECTION_TERMINAL_DOT_STYLES}</style>
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
            <span className={dotClassForStatus(overlayStatus, true)} aria-hidden />
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
      <style>{CONNECTION_TERMINAL_DOT_STYLES}</style>
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
