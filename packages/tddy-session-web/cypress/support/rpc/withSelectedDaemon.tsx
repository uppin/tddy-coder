/**
 * Test-only wrappers stating which daemons the directory lists and who is in the common room, around
 * a drawer surface mounted into `./drawerWorld`.
 *
 * Each wrapper nests a clients port (daemon selection replaced) and a presence double (the common
 * room's participants) inside whichever mount supplied the transports, so a spec states the world
 * the way it did when the app's `SelectedDaemonProvider` stood here.
 */

import React, { useMemo, useState } from "react";
import {
  ClientsProvider,
  PresencePortProvider,
  inferParticipantRole,
  useClientsPort,
  type DaemonHost,
  type RoomHandle,
  type RoomParticipant,
} from "tddy-client-web";
import { ConnectionProviderRegistry } from "tddy-client-web/connections/registry";
import { aPresenceDouble } from "tddy-client-web/testkit";

/** Default single-daemon fixture — the "one local daemon" shape used across the specs. */
export const DEFAULT_TEST_DAEMON: DaemonHost = { instanceId: "local", label: "local (this daemon)" };

interface FakeCommonRoom {
  readonly participants: ReadonlyArray<{ identity: string; metadata: string }>;
}

/** A common room carrying remote participants with the given identities (a coder is `daemon-<instanceId>-<sessionId>`). */
export function aFakeCommonRoom(participantIdentities: string[]): RoomHandle {
  return { participants: participantIdentities.map((identity) => ({ identity, metadata: "" })) } as unknown as RoomHandle;
}

/** A common room this page has joined — what a connected fleet looks like. */
export function aJoinedCommonRoom(participantIdentities: string[] = []): RoomHandle {
  return aFakeCommonRoom(participantIdentities);
}

/** Like {@link aFakeCommonRoom}, each participant carrying its own `metadata` string. */
export function aFakeCommonRoomWithMetadata(
  participants: ReadonlyArray<{ identity: string; metadata: string }>,
): RoomHandle {
  return { participants } as unknown as RoomHandle;
}

function toParticipant({ identity, metadata }: { identity: string; metadata: string }): RoomParticipant {
  return { identity, role: inferParticipantRole(identity, metadata), joinedAt: null, metadata, codexOAuth: null };
}

function SelectedDaemonWorld({
  daemons,
  room,
  servingInstanceId,
  children,
}: {
  daemons: DaemonHost[];
  room: RoomHandle | null;
  servingInstanceId?: string;
  children?: React.ReactNode;
}) {
  const outer = useClientsPort();
  const [selectedInstanceId, selectDaemon] = useState<string | null>(daemons[0]?.instanceId ?? null);
  // Only the listed daemons are reachable: a host the directory does not list has no wire to it.
  const connections = useMemo(() => {
    const listed = new ConnectionProviderRegistry();
    listed.register({
      id: "listed-daemons",
      connectHost: (hostId) => (daemons.some((d) => d.instanceId === hostId) ? outer.connections.connectHost(hostId) : null),
    });
    return listed;
  }, [outer, daemons]);
  const clients = useMemo(
    () => ({
      ...outer,
      connections,
      selectedDaemon: {
        ...outer.selectedDaemon,
        daemons,
        selectedInstanceId,
        selectedLabel: daemons.find((d) => d.instanceId === selectedInstanceId)?.label ?? null,
        servingInstanceId,
        selectDaemon,
      },
    }),
    [outer, connections, daemons, selectedInstanceId, servingInstanceId],
  );
  const presence = useMemo(() => {
    const double = aPresenceDouble();
    const common = double.room("common-room");
    for (const daemon of daemons) double.presentHost(daemon.instanceId, "common-room");
    double.setParticipants(common, ((room as unknown as FakeCommonRoom | null)?.participants ?? []).map(toParticipant));
    return double;
  }, [daemons, room]);
  return (
    <ClientsProvider port={clients}>
      <PresencePortProvider port={presence}>{children}</PresencePortProvider>
    </ClientsProvider>
  );
}

export function withSelectedDaemon(
  children: React.ReactNode,
  daemons: DaemonHost[] = [DEFAULT_TEST_DAEMON],
  participantIdentities?: string[],
): React.ReactElement {
  const room = participantIdentities !== undefined ? aFakeCommonRoom(participantIdentities) : null;
  return (
    <SelectedDaemonWorld daemons={daemons} room={room}>
      {children}
    </SelectedDaemonWorld>
  );
}

/** Variant that also names the daemon that served the web bundle — the one host the browser's own HTTP transport reaches. */
export function withSelectedDaemonServedBy(
  children: React.ReactNode,
  daemons: DaemonHost[],
  servingInstanceId: string,
): React.ReactElement {
  return (
    <SelectedDaemonWorld daemons={daemons} room={null} servingInstanceId={servingInstanceId}>
      {children}
    </SelectedDaemonWorld>
  );
}

/** Variant taking a pre-built common room (e.g. {@link aFakeCommonRoomWithMetadata}) to seed participant metadata. */
export function withSelectedDaemonRoom(
  children: React.ReactNode,
  daemons: DaemonHost[],
  room: RoomHandle,
): React.ReactElement {
  return (
    <SelectedDaemonWorld daemons={daemons} room={room}>
      {children}
    </SelectedDaemonWorld>
  );
}
