import { useState } from "react";
import type { CommonRoomStatus } from "../hooks/useCommonRoom";
import { shouldShowParticipantVideoAffordance } from "../hooks/participantCameraVideo";
import type { RoomParticipant } from "../hooks/useRoomParticipants";
import { ParticipantVideoPreviewDialog } from "./ParticipantVideoPreviewDialog";
import { Button } from "./ui/button";

/** Inline SVG (Lucide `video` paths) — avoids lucide-react context issues in Cypress CT. */
function VideoCameraIcon({ className }: { className?: string }) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden
    >
      <path d="m16 13 5.223 3.482a.5.5 0 0 0 .777-.416V7.87a.5.5 0 0 0-.752-.432L16 10.5" />
      <rect x="2" y="6" width="14" height="12" rx="2" />
    </svg>
  );
}

function safeTestIdPart(s: string): string {
  return s.replace(/[^a-zA-Z0-9_-]/g, "_");
}

function formatJoinedAt(ms: number | null): string {
  if (ms === null) return "—";
  try {
    return new Date(ms).toLocaleString();
  } catch {
    return "—";
  }
}

const tableStyle = {
  width: "100%",
  borderCollapse: "collapse" as const,
  marginTop: 8,
  fontSize: 13,
};

const roleBadge = (role: RoomParticipant["role"]) => {
  const colors: Record<RoomParticipant["role"], string> = {
    browser: "#1565c0",
    server: "#2e7d32",
    unknown: "#666",
  };
  return (
    <span
      style={{
        display: "inline-block",
        padding: "2px 6px",
        borderRadius: 4,
        fontSize: 11,
        fontWeight: 600,
        color: "#fff",
        backgroundColor: colors[role],
      }}
    >
      {role}
    </span>
  );
};

export interface ParticipantListProps {
  participants: RoomParticipant[];
  roomStatus: CommonRoomStatus;
  connectionError: string | null;
  /**
   * Optional map of LiveKit identity → whether that participant exposes renderable camera video.
   * Used by tests and until ConnectionScreen plumbs live track state from the Room.
   */
  participantHasCameraVideo?: Record<string, boolean>;
}

/**
 * LiveKit presence list for the shared common room (daemon `livekit.common_room`).
 */
export function ParticipantList({
  participants,
  roomStatus,
  connectionError,
  participantHasCameraVideo,
}: ParticipantListProps) {
  const [videoPreviewIdentity, setVideoPreviewIdentity] = useState<string | null>(null);

  if (roomStatus === "idle" || roomStatus === "connecting") {
    return (
      <div data-testid="participant-list" data-room-status="connecting">
        <p style={{ fontSize: 14, color: "#555" }}>Connecting to presence room…</p>
      </div>
    );
  }

  if (roomStatus === "error") {
    return (
      <div data-testid="participant-list" data-room-status="error">
        <p style={{ fontSize: 14, color: "#c00" }} data-testid="participant-list-error">
          {connectionError ?? "Failed to join presence room."}
        </p>
      </div>
    );
  }

  if (participants.length === 0) {
    return (
      <div data-testid="participant-list" data-room-status="connected">
        <p style={{ fontSize: 14, color: "#666" }} data-testid="participant-list-empty">
          No other participants in this room.
        </p>
      </div>
    );
  }

  console.debug("[tddy-web:participant-video] ParticipantList: rendering connected table", {
    participantCount: participants.length,
    hasCameraMap: participantHasCameraVideo !== undefined,
  });

  return (
    <div data-testid="participant-list" data-room-status="connected">
      <table style={tableStyle}>
        <thead>
          <tr style={{ borderBottom: "1px solid #ccc", textAlign: "left" }}>
            <th style={{ padding: 6 }}>Identity</th>
            <th style={{ padding: 6 }}>Role</th>
            <th style={{ padding: 6 }}>Joined</th>
            <th style={{ padding: 6 }}>Metadata</th>
            <th style={{ padding: 6 }}>Video</th>
          </tr>
        </thead>
        <tbody>
          {participants.map((p) => {
            const id = safeTestIdPart(p.identity);
            const showVideoAffordance = shouldShowParticipantVideoAffordance(
              participantHasCameraVideo,
              p.identity,
            );
            return (
              <tr key={p.identity} style={{ borderBottom: "1px solid #eee" }}>
                <td style={{ padding: 6 }} data-testid={`participant-entry-${id}`}>
                  {p.identity}
                </td>
                <td style={{ padding: 6 }} data-testid={`participant-role-${id}`}>
                  {roleBadge(p.role)}
                </td>
                <td style={{ padding: 6 }} data-testid={`participant-joined-${id}`}>
                  {formatJoinedAt(p.joinedAt)}
                </td>
                <td
                  style={{ padding: 6, maxWidth: 200, wordBreak: "break-all" }}
                  data-testid={`participant-metadata-${id}`}
                >
                  {p.metadata || "—"}
                </td>
                <td style={{ padding: 6 }} data-testid={`participant-video-cell-${id}`}>
                  {showVideoAffordance ? (
                    <Button
                      type="button"
                      variant="outline"
                      size="icon-xs"
                      data-testid={`participant-video-trigger-${id}`}
                      aria-label={`Open video preview for ${p.identity}`}
                      onClick={() => {
                        console.info("[tddy-web:participant-video] ParticipantList: open video preview", {
                          identity: p.identity,
                        });
                        setVideoPreviewIdentity(p.identity);
                      }}
                    >
                      <VideoCameraIcon className="size-3.5" />
                    </Button>
                  ) : null}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>

      <ParticipantVideoPreviewDialog
        identity={videoPreviewIdentity ?? ""}
        open={videoPreviewIdentity !== null}
        onOpenChange={(next) => {
          if (!next) {
            console.info("[tddy-web:participant-video] ParticipantList: video preview closed");
            setVideoPreviewIdentity(null);
          }
        }}
      />
    </div>
  );
}
