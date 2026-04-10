import { ExternalLink } from "lucide-react";
import type { CommonRoomStatus } from "../hooks/useCommonRoom";
import type { RoomParticipant } from "../hooks/useRoomParticipants";

/** LiveKit participant metadata JSON published when Codex triggers OAuth (see tddy-livekit poller). */
export function parseCodexOAuthPending(metadata: string): { authorizeUrl: string } | null {
  const t = metadata.trim();
  if (!t.startsWith("{")) return null;
  try {
    const o = JSON.parse(t) as {
      codex_oauth?: { pending?: boolean; authorize_url?: string };
    };
    const u = o.codex_oauth?.authorize_url;
    if (o.codex_oauth?.pending === true && typeof u === "string" && u.startsWith("https://")) {
      return { authorizeUrl: u };
    }
  } catch {
    return null;
  }
  return null;
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
}

/**
 * LiveKit presence list for the shared common room (daemon `livekit.common_room`).
 */
export function ParticipantList({ participants, roomStatus, connectionError }: ParticipantListProps) {
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

  return (
    <div data-testid="participant-list" data-room-status="connected">
      <table style={tableStyle}>
        <thead>
          <tr style={{ borderBottom: "1px solid #ccc", textAlign: "left" }}>
            <th style={{ padding: 6 }}>Identity</th>
            <th style={{ padding: 6 }}>Role</th>
            <th style={{ padding: 6 }}>Joined</th>
            <th style={{ padding: 6 }}>Metadata</th>
            <th style={{ padding: 6 }}>Codex sign-in</th>
          </tr>
        </thead>
        <tbody>
          {participants.map((p) => {
            const id = safeTestIdPart(p.identity);
            const codexOAuth = parseCodexOAuthPending(p.metadata);
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
                  {codexOAuth ? (
                    <span title={p.metadata}>Codex OAuth pending — open sign-in (→)</span>
                  ) : (
                    (p.metadata || "—")
                  )}
                </td>
                <td style={{ padding: 6, textAlign: "center" }} data-testid={`participant-codex-oauth-${id}`}>
                  {codexOAuth ? (
                    <a
                      href={codexOAuth.authorizeUrl}
                      target="_blank"
                      rel="noopener noreferrer"
                      title="Open Codex / OpenAI authorization in a new tab"
                      aria-label="Open Codex OAuth in new tab"
                      style={{
                        display: "inline-flex",
                        alignItems: "center",
                        justifyContent: "center",
                        color: "#1565c0",
                      }}
                    >
                      <ExternalLink size={18} strokeWidth={2} />
                    </a>
                  ) : (
                    "—"
                  )}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
