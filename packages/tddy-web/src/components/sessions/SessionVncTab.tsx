/**
 * VNC tab content in the session inspector drawer.
 *
 * Shows a list of VNC targets configured for the session and an Add form.
 * When a password is provided on the Add form, prompts for a vault passphrase
 * before calling AddVncTarget.
 */

import React, { useEffect, useReducer, useState } from "react";
import type { Room } from "livekit-client";
import { VncPassphraseDialog } from "./VncPassphraseDialog";
import { VncOverlay } from "./VncOverlay";
import { applyVncTabAction, initialVncTabState } from "./vncTabState";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface SessionVncTabProps {
  sessionId: string;
  sessionToken: string;
  room: Room | null;
  onListVncTargets: () => Promise<VncTargetInfo[]>;
  onAddVncTarget: (req: AddVncTargetReq) => Promise<VncTargetInfo>;
  onRemoveVncTarget: (targetId: string) => Promise<void>;
  onUnlockVncVault: (passphrase: string) => Promise<void>;
  onStartVncStream: (targetId: string) => Promise<StartVncStreamResult>;
  onStopVncStream: (targetId: string) => Promise<void>;
}

export interface VncTargetInfo {
  id: string;
  label: string;
  host: string;
  port: number;
}

export interface AddVncTargetReq {
  label: string;
  host: string;
  port: number;
  password: string;
}

export interface StartVncStreamResult {
  livekitRoom: string;
  livekitUrl: string;
  bridgeIdentity: string;
  trackName: string;
  width: number;
  height: number;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function SessionVncTab({
  room,
  onListVncTargets,
  onAddVncTarget,
  onRemoveVncTarget,
  onUnlockVncVault,
  onStartVncStream,
  onStopVncStream,
}: SessionVncTabProps): React.ReactElement {
  const [state, dispatch] = useReducer(applyVncTabAction, initialVncTabState);

  // Add form fields
  const [label, setLabel] = useState("");
  const [host, setHost] = useState("");
  const [port, setPort] = useState("5900");
  const [password, setPassword] = useState("");

  // Pending add request — held while passphrase dialog is open
  const [pendingAdd, setPendingAdd] = useState<AddVncTargetReq | null>(null);
  const [passphraseOpen, setPassphraseOpen] = useState(false);

  // Active stream result — stored when StartVncStream succeeds
  const [activeStreamResult, setActiveStreamResult] = useState<StartVncStreamResult | null>(null);

  useEffect(() => {
    onListVncTargets()
      .then((targets) => dispatch({ type: "set_targets", targets }))
      .catch(() => {/* ignore errors in list — non-fatal */});
  // onListVncTargets is a callback prop; the effect intentionally runs once on mount.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const req: AddVncTargetReq = {
      label,
      host,
      port: parseInt(port, 10) || 5900,
      password,
    };

    if (password) {
      // Vault unlock required before adding a target with a password
      setPendingAdd(req);
      setPassphraseOpen(true);
    } else {
      onAddVncTarget(req)
        .then((target) => {
          dispatch({ type: "add_target", target });
          setLabel("");
          setHost("");
          setPort("5900");
          setPassword("");
        })
        .catch(() => {/* errors handled silently for now */});
    }
  };

  const handlePassphraseConfirm = (passphrase: string) => {
    setPassphraseOpen(false);
    if (!pendingAdd) return;
    const req = pendingAdd;
    setPendingAdd(null);

    onUnlockVncVault(passphrase)
      .then(() => onAddVncTarget(req))
      .then((target) => {
        dispatch({ type: "add_target", target });
        setLabel("");
        setHost("");
        setPort("5900");
        setPassword("");
      })
      .catch(() => {/* errors handled silently for now */});
  };

  const handlePassphraseCancel = () => {
    setPassphraseOpen(false);
    setPendingAdd(null);
  };

  const handleStart = (targetId: string) => {
    dispatch({ type: "set_stream_status", targetId, status: "starting" });
    onStartVncStream(targetId)
      .then((result) => {
        dispatch({ type: "set_stream_status", targetId, status: "streaming" });
        dispatch({ type: "open_overlay", targetId });
        setActiveStreamResult(result);
      })
      .catch(() => {
        dispatch({ type: "set_stream_status", targetId, status: "error" });
      });
  };

  const handleOverlayClose = () => {
    const targetId = state.activeOverlayTargetId;
    dispatch({ type: "close_overlay" });
    setActiveStreamResult(null);
    if (targetId) {
      dispatch({ type: "set_stream_status", targetId, status: "stopped" });
      onStopVncStream(targetId).catch(() => {/* ignore */});
    }
  };

  const handleRemove = (targetId: string) => {
    onRemoveVncTarget(targetId)
      .then(() => dispatch({ type: "remove_target", targetId }))
      .catch(() => {/* errors handled silently for now */});
  };

  return (
    <>
      <div data-testid="sessions-vnc-tab-panel" className="flex flex-col gap-3 px-3 py-3">
        <div data-testid="sessions-vnc-target-list" className="flex flex-col gap-1">
          {state.targets.map((t) => (
            <div
              key={t.id}
              data-testid={`sessions-vnc-target-row-${t.id}`}
              className="flex items-center justify-between border border-border rounded px-2 py-1 gap-2"
            >
              <span className="text-xs flex-1 truncate">
                {t.label} — {t.host}:{t.port}
              </span>
              <div className="flex items-center gap-1">
                <button
                  data-testid={`sessions-vnc-start-${t.id}`}
                  type="button"
                  className="px-2 py-0.5 text-xs bg-foreground text-background rounded hover:opacity-90"
                  onClick={() => handleStart(t.id)}
                >
                  Start
                </button>
                <button
                  data-testid={`sessions-vnc-remove-${t.id}`}
                  type="button"
                  className="px-2 py-0.5 text-xs border border-destructive text-destructive rounded hover:bg-destructive/10"
                  onClick={() => handleRemove(t.id)}
                >
                  Remove
                </button>
              </div>
            </div>
          ))}
        </div>

        <form
          data-testid="sessions-vnc-add-form"
          onSubmit={handleSubmit}
          className="flex flex-col gap-2"
        >
          <input
            data-testid="sessions-vnc-add-label"
            type="text"
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            placeholder="Label"
            className="border border-border rounded px-2 py-1 text-xs bg-background"
          />
          <input
            data-testid="sessions-vnc-add-host"
            type="text"
            value={host}
            onChange={(e) => setHost(e.target.value)}
            placeholder="Host"
            className="border border-border rounded px-2 py-1 text-xs bg-background"
          />
          <input
            data-testid="sessions-vnc-add-port"
            type="text"
            value={port}
            onChange={(e) => setPort(e.target.value)}
            placeholder="Port"
            className="border border-border rounded px-2 py-1 text-xs bg-background"
          />
          <input
            data-testid="sessions-vnc-add-password"
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            placeholder="Password (optional)"
            className="border border-border rounded px-2 py-1 text-xs bg-background"
          />
          <button
            data-testid="sessions-vnc-add-submit"
            type="submit"
            className="px-3 py-1 text-xs bg-foreground text-background rounded hover:opacity-90"
          >
            Add target
          </button>
        </form>

        <VncPassphraseDialog
          open={passphraseOpen}
          vaultExists={false}
          onConfirm={handlePassphraseConfirm}
          onCancel={handlePassphraseCancel}
        />
      </div>

      {state.activeOverlayTargetId && activeStreamResult && (
        <VncOverlay
          room={room}
          bridgeIdentity={activeStreamResult.bridgeIdentity}
          trackName={activeStreamResult.trackName}
          width={activeStreamResult.width}
          height={activeStreamResult.height}
          onClose={handleOverlayClose}
        />
      )}
    </>
  );
}
