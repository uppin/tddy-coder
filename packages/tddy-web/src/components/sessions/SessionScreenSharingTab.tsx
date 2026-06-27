/**
 * Screen Sharing tab content in the session inspector drawer.
 *
 * Shows a list of screen-sharing targets configured for the session and an Add form.
 * The form includes a protocol selector (VNC / RDP) that auto-fills the default port.
 * When a password is provided on the Add form, prompts for a vault passphrase
 * before calling AddTarget.
 */

import React, { useEffect, useReducer, useState } from "react";
import type { Room } from "livekit-client";
import { Protocol } from "../../gen/screen_sharing_pb";
import { ScreenSharingPassphraseDialog } from "./ScreenSharingPassphraseDialog";
import { ScreenSharingOverlay } from "./ScreenSharingOverlay";
import { applyScreenSharingTabAction, initialScreenSharingTabState } from "./screenSharingTabState";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface SessionScreenSharingTabProps {
  sessionId: string;
  sessionToken: string;
  room: Room | null;
  onListTargets: () => Promise<ScreenSharingTargetInfo[]>;
  onAddTarget: (req: AddScreenSharingTargetReq) => Promise<ScreenSharingTargetInfo>;
  onRemoveTarget: (targetId: string) => Promise<void>;
  onUnlockVault: (passphrase: string) => Promise<void>;
  onStartStream: (targetId: string) => Promise<StartStreamResult>;
  onStopStream: (targetId: string) => Promise<void>;
}

export interface ScreenSharingTargetInfo {
  id: string;
  label: string;
  host: string;
  port: number;
  protocol: Protocol;
  username: string;
}

export interface AddScreenSharingTargetReq {
  label: string;
  host: string;
  port: number;
  username: string;
  password: string;
  protocol: Protocol;
}

export interface StartStreamResult {
  livekitRoom: string;
  livekitUrl: string;
  bridgeIdentity: string;
  trackName: string;
  width: number;
  height: number;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const PROTOCOL_OPTIONS: { value: Protocol; label: string; defaultPort: number }[] = [
  { value: Protocol.VNC, label: "VNC", defaultPort: 5900 },
  { value: Protocol.RDP, label: "RDP", defaultPort: 3389 },
];

function defaultPortForProtocol(p: Protocol): number {
  return PROTOCOL_OPTIONS.find((o) => o.value === p)?.defaultPort ?? 5900;
}

function protocolLabel(p: Protocol): string {
  return p === Protocol.RDP ? "RDP" : "VNC";
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function SessionScreenSharingTab({
  room,
  onListTargets,
  onAddTarget,
  onRemoveTarget,
  onUnlockVault,
  onStartStream,
  onStopStream,
}: SessionScreenSharingTabProps): React.ReactElement {
  const [state, dispatch] = useReducer(applyScreenSharingTabAction, initialScreenSharingTabState);

  // Add form fields
  const [label, setLabel] = useState("");
  const [host, setHost] = useState("");
  const [protocol, setProtocol] = useState<Protocol>(Protocol.VNC);
  const [port, setPort] = useState("5900");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");

  // Pending add request — held while passphrase dialog is open
  const [pendingAdd, setPendingAdd] = useState<AddScreenSharingTargetReq | null>(null);
  const [passphraseOpen, setPassphraseOpen] = useState(false);
  // Tracks whether UnlockVault has succeeded in this session — skips re-prompting.
  const [vaultUnlocked, setVaultUnlocked] = useState(false);

  // Active stream result — stored when StartStream succeeds
  const [activeStreamResult, setActiveStreamResult] = useState<StartStreamResult | null>(null);
  // Transient error message shown below the form.
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  useEffect(() => {
    onListTargets()
      .then((targets) =>
        dispatch({
          type: "set_targets",
          targets: targets.map((t) => ({
            ...t,
            protocol: t.protocol === Protocol.RDP ? "rdp" : "vnc",
          })),
        }),
      )
      .catch(() => {/* ignore list errors — non-fatal */});
  // onListTargets is a callback prop; the effect intentionally runs once on mount.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const handleProtocolChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
    const p = e.target.value === "RDP" ? Protocol.RDP : Protocol.VNC;
    setProtocol(p);
    setPort(String(defaultPortForProtocol(p)));
  };

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const req: AddScreenSharingTargetReq = {
      label,
      host,
      port: parseInt(port, 10) || defaultPortForProtocol(protocol),
      username,
      password,
      protocol,
    };

    if (password && !vaultUnlocked) {
      setPendingAdd(req);
      setPassphraseOpen(true);
    } else {
      submitAdd(req);
    }
  };

  const submitAdd = (req: AddScreenSharingTargetReq) => {
    setErrorMsg(null);
    onAddTarget(req)
      .then((target) => {
        dispatch({
          type: "add_target",
          target: {
            ...target,
            protocol: target.protocol === Protocol.RDP ? "rdp" : "vnc",
          },
        });
        setLabel("");
        setHost("");
        setProtocol(Protocol.VNC);
        setPort("5900");
        setUsername("");
        setPassword("");
      })
      .catch((e: unknown) => {
        setErrorMsg(e instanceof Error ? e.message : "Failed to add target");
      });
  };

  const handlePassphraseConfirm = (passphrase: string) => {
    setPassphraseOpen(false);
    if (!pendingAdd) return;
    const req = pendingAdd;
    setPendingAdd(null);

    onUnlockVault(passphrase)
      .then(() => {
        setVaultUnlocked(true);
        submitAdd(req);
      })
      .catch((e: unknown) => {
        setErrorMsg(e instanceof Error ? e.message : "Failed to unlock vault — wrong passphrase?");
      });
  };

  const handlePassphraseCancel = () => {
    setPassphraseOpen(false);
    setPendingAdd(null);
  };

  const handleStart = (targetId: string) => {
    setErrorMsg(null);
    dispatch({ type: "set_stream_status", targetId, status: "starting" });
    onStartStream(targetId)
      .then((result) => {
        dispatch({ type: "set_stream_status", targetId, status: "streaming" });
        dispatch({ type: "open_overlay", targetId });
        setActiveStreamResult(result);
      })
      .catch((e: unknown) => {
        dispatch({ type: "set_stream_status", targetId, status: "error" });
        setErrorMsg(e instanceof Error ? e.message : "Failed to start stream");
      });
  };

  const handleOverlayClose = () => {
    const targetId = state.activeOverlayTargetId;
    dispatch({ type: "close_overlay" });
    setActiveStreamResult(null);
    if (targetId) {
      dispatch({ type: "set_stream_status", targetId, status: "stopped" });
      onStopStream(targetId).catch(() => {/* ignore */});
    }
  };

  const handleRemove = (targetId: string) => {
    onRemoveTarget(targetId)
      .then(() => dispatch({ type: "remove_target", targetId }))
      .catch(() => {/* errors handled silently for now */});
  };

  return (
    <>
      <div data-testid="sessions-screen-sharing-tab-panel" className="flex flex-col gap-3 px-3 py-3">
        <div data-testid="sessions-screen-sharing-target-list" className="flex flex-col gap-1">
          {state.targets.map((t) => (
            <div
              key={t.id}
              data-testid={`sessions-screen-sharing-target-row-${t.id}`}
              className="flex items-center justify-between border border-border rounded px-2 py-1 gap-2"
            >
              <span className="text-xs flex-1 truncate">
                {t.label} — {t.host}:{t.port} ({t.protocol.toUpperCase()})
              </span>
              <div className="flex items-center gap-1">
                <button
                  data-testid={`sessions-screen-sharing-start-${t.id}`}
                  type="button"
                  className="px-2 py-0.5 text-xs bg-foreground text-background rounded hover:opacity-90"
                  onClick={() => handleStart(t.id)}
                >
                  Start
                </button>
                <button
                  data-testid={`sessions-screen-sharing-remove-${t.id}`}
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
          data-testid="sessions-screen-sharing-add-form"
          onSubmit={handleSubmit}
          className="flex flex-col gap-2"
        >
          <input
            data-testid="sessions-screen-sharing-add-label"
            type="text"
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            placeholder="Label"
            className="border border-border rounded px-2 py-1 text-xs bg-background"
          />
          <input
            data-testid="sessions-screen-sharing-add-host"
            type="text"
            value={host}
            onChange={(e) => setHost(e.target.value)}
            placeholder="Host"
            className="border border-border rounded px-2 py-1 text-xs bg-background"
          />
          <select
            data-testid="sessions-screen-sharing-add-protocol"
            value={protocolLabel(protocol)}
            onChange={handleProtocolChange}
            className="border border-border rounded px-2 py-1 text-xs bg-background"
          >
            {PROTOCOL_OPTIONS.map((opt) => (
              <option key={opt.label} value={opt.label}>
                {opt.label}
              </option>
            ))}
          </select>
          {protocol === Protocol.RDP && (
            <input
              data-testid="sessions-screen-sharing-add-username"
              type="text"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              placeholder="Username"
              className="border border-border rounded px-2 py-1 text-xs bg-background"
            />
          )}
          <input
            data-testid="sessions-screen-sharing-add-port"
            type="text"
            value={port}
            onChange={(e) => setPort(e.target.value)}
            placeholder={String(defaultPortForProtocol(protocol))}
            className="border border-border rounded px-2 py-1 text-xs bg-background"
          />
          <input
            data-testid="sessions-screen-sharing-add-password"
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            placeholder="Password (optional)"
            className="border border-border rounded px-2 py-1 text-xs bg-background"
          />
          <button
            data-testid="sessions-screen-sharing-add-submit"
            type="submit"
            className="px-3 py-1 text-xs bg-foreground text-background rounded hover:opacity-90"
          >
            Add target
          </button>
        </form>

        {errorMsg && (
          <p className="text-xs text-destructive px-1" role="alert">
            {errorMsg}
          </p>
        )}

        <ScreenSharingPassphraseDialog
          open={passphraseOpen}
          vaultExists={false}
          onConfirm={handlePassphraseConfirm}
          onCancel={handlePassphraseCancel}
        />
      </div>

      {state.activeOverlayTargetId && activeStreamResult && (
        <ScreenSharingOverlay
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
