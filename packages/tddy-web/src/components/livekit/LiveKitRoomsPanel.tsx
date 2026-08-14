/**
 * The LiveKit rooms & participants panel on `#/livekit`: every room on the LiveKit server, and the
 * participants joined to each.
 *
 * Sits below the connected-participants panel and answers a different question — that one shows the
 * one room this browser joined, as seen by the client SDK; this one shows the server's own view of
 * every room. Rooms render expanded, because "who is in there" is the whole point of the screen.
 *
 * PRD: `docs/ft/web/livekit-rooms-panel.md`
 */

import { useEffect, useState } from "react";
import { Tooltip, TooltipContent, TooltipTrigger } from "../ui/tooltip";
import type { LiveKitRoom, LiveKitRoomParticipant } from "../../lib/liveKitRoomsState";
import { metadataCardText } from "../../lib/liveKitMetadataCard";
import { safeTestIdPart } from "../../lib/testId";
import { useLiveKitRooms } from "../../rpc/useLiveKitRooms";

function formatInstant(ms: number): string {
  return new Date(ms).toLocaleString();
}

export function LiveKitRoomsPanel() {
  const { rooms, hasSnapshot, error } = useLiveKitRooms();
  const [collapsed, setCollapsed] = useState<string[]>([]);

  // A room that leaves takes its collapsed flag with it. Without this the list only ever grows over
  // a long-lived session, and a room recreated under a name someone once collapsed would come back
  // collapsed even though the panel renders rooms expanded.
  useEffect(() => {
    setCollapsed((prev) => {
      const stillKnown = prev.filter((name) => rooms.some((room) => room.name === name));
      return stillKnown.length === prev.length ? prev : stillKnown;
    });
  }, [rooms]);

  const toggleRoom = (room: string) =>
    setCollapsed((prev) =>
      prev.includes(room) ? prev.filter((name) => name !== room) : [...prev, room],
    );

  return (
    <div data-testid="livekit-rooms-panel" className="mt-3 rounded-md border border-border p-3">
      <h3 className="mt-0 text-base font-semibold">Rooms</h3>

      {error !== null && (
        <p data-testid="livekit-rooms-panel-error" className="text-sm text-destructive">
          {error}
        </p>
      )}

      {!hasSnapshot && error === null && (
        <p data-testid="livekit-rooms-panel-loading" className="text-sm text-muted-foreground">
          Loading rooms…
        </p>
      )}

      {hasSnapshot && rooms.length === 0 && (
        <p data-testid="livekit-rooms-panel-empty" className="text-sm text-muted-foreground">
          No rooms on the LiveKit server.
        </p>
      )}

      {rooms.map((room) => (
        <RoomRow
          key={room.name}
          room={room}
          expanded={!collapsed.includes(room.name)}
          onToggle={() => toggleRoom(room.name)}
        />
      ))}
    </div>
  );
}

function RoomRow({
  room,
  expanded,
  onToggle,
}: {
  room: LiveKitRoom;
  expanded: boolean;
  onToggle: () => void;
}) {
  const id = safeTestIdPart(room.name);

  return (
    <div
      data-testid={`livekit-room-entry-${id}`}
      data-room-name={room.name}
      className="mt-2 rounded-md border border-border p-2"
    >
      <div className="flex items-center gap-2 text-sm">
        <button
          type="button"
          data-testid={`livekit-room-toggle-${id}`}
          aria-expanded={expanded}
          aria-label={`${expanded ? "Collapse" : "Expand"} room ${room.name}`}
          onClick={onToggle}
          className="rounded border border-border px-1.5 leading-none"
        >
          {expanded ? "▾" : "▸"}
        </button>
        <span data-testid={`livekit-room-name-${id}`} className="font-medium">
          {room.name}
        </span>
        {room.label !== null && (
          <span data-testid={`livekit-room-label-${id}`} className="text-muted-foreground">
            {room.label}
          </span>
        )}
        <span className="text-muted-foreground">participants:</span>
        <span data-testid={`livekit-room-participant-count-${id}`} className="tabular-nums">
          {room.participants.length}
        </span>
        <span className="text-muted-foreground">created:</span>
        <span data-testid={`livekit-room-created-at-${id}`}>{formatInstant(room.createdAtMs)}</span>
      </div>

      {expanded && room.participants.length === 0 && (
        <p
          data-testid={`livekit-room-no-participants-${id}`}
          className="mt-1 text-sm text-muted-foreground"
        >
          No participants joined.
        </p>
      )}

      {expanded &&
        room.participants.map((participant) => (
          <ParticipantRow key={participant.identity} room={room.name} participant={participant} />
        ))}
    </div>
  );
}

/**
 * One participant, with its metadata behind a card that opens on pointer-hover and on keyboard
 * focus — metadata reachable only by pointer is unreachable by keyboard.
 *
 * The card's `aria-label` also replaces the copy of its children that Radix renders visually hidden
 * for screen readers, which keeps the metadata text in the DOM exactly once.
 */
function ParticipantRow({
  room,
  participant,
}: {
  room: string;
  participant: LiveKitRoomParticipant;
}) {
  const id = `${safeTestIdPart(room)}-${safeTestIdPart(participant.identity)}`;

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <div
          data-testid={`livekit-room-participant-entry-${id}`}
          data-participant-identity={participant.identity}
          tabIndex={0}
          className="mt-1 flex items-center gap-2 rounded px-1 text-sm"
        >
          <span>{participant.identity}</span>
          <span data-testid={`livekit-room-participant-role-${id}`} className="text-muted-foreground">
            {participant.role}
          </span>
          <span className="text-muted-foreground">joined:</span>
          <span data-testid={`livekit-room-participant-joined-${id}`}>
            {formatInstant(participant.joinedAtMs)}
          </span>
          <span data-testid={`livekit-room-participant-state-${id}`} className="text-muted-foreground">
            {participant.state}
          </span>
        </div>
      </TooltipTrigger>
      {/* Anchored below the row: a participant row spans the panel's full width, so a card placed
          beside it would hang off the edge of the viewport. */}
      <TooltipContent
        side="bottom"
        align="start"
        aria-label={`Metadata for ${participant.identity}`}
      >
        <pre
          data-testid={`livekit-room-participant-metadata-${id}`}
          className="max-h-64 max-w-md overflow-auto whitespace-pre-wrap break-all font-mono text-xs"
        >
          {metadataCardText(participant.metadata)}
        </pre>
      </TooltipContent>
    </Tooltip>
  );
}
