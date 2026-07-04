/**
 * Shared common-room daemon-selection context.
 *
 * A `tddy-daemon` joins the common room as two participants (see `participantRole.ts`'s
 * `daemonRpcIdentity` doc comment): the selector lists daemons by their discovery identity, but
 * daemon-level RPC (`ConnectionService`, `TaskService`, `VmService`, …) must address
 * `daemon-{instanceId}`. `SelectedDaemonProvider` owns the one common-room connection shared by
 * every daemon-mode screen, the currently selected daemon, and `useDaemonClient` — the daemon-level
 * equivalent of `useHttpClient`/`useLiveKitClient` from `./transportProvider`.
 *
 * PRD: docs/ft/web/daemon-selector-livekit-rpc.md.
 */

import {
  createContext,
  Fragment,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import type { Client } from "@connectrpc/connect";
import type { DescService } from "@bufbuild/protobuf";
import type { Room } from "livekit-client";
import { useAuth } from "../hooks/useAuth";
import { useCommonRoom } from "../hooks/useCommonRoom";
import { useRoomParticipants } from "../hooks/useRoomParticipants";
import { daemonHostsFromParticipants, daemonRpcIdentity, type DaemonHost } from "../lib/participantRole";
import { presenceIdentityForUser } from "../lib/presenceIdentity";
import { useLiveKitClient } from "./transportProvider";

// ---------------------------------------------------------------------------
// Session-scoped persistence
// ---------------------------------------------------------------------------

const SELECTED_DAEMON_STORAGE_KEY = "tddy_selected_daemon";

/** The selected daemon's instance id, persisted for this browser tab, or `null` if never set. */
export function readStoredSelectedDaemon(): string | null {
  return sessionStorage.getItem(SELECTED_DAEMON_STORAGE_KEY);
}

/** Persist the selected daemon's instance id for this browser tab. */
export function writeStoredSelectedDaemon(instanceId: string): void {
  sessionStorage.setItem(SELECTED_DAEMON_STORAGE_KEY, instanceId);
}

// ---------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------

/**
 * Resolve which daemon should be selected, in precedence order:
 * 1. `storedInstanceId`, if it is still among `daemons`.
 * 2. `servingInstanceId` (the daemon that served this web bundle), if still among `daemons`.
 * 3. The first daemon in `daemons`, if any.
 * 4. `null`, when there are no daemons in the room yet.
 */
export function resolveSelectedDaemonInstanceId(params: {
  daemons: DaemonHost[];
  servingInstanceId?: string;
  storedInstanceId?: string | null;
}): string | null {
  const { daemons, servingInstanceId, storedInstanceId } = params;
  const presentIds = new Set(daemons.map((d) => d.instanceId));
  if (storedInstanceId && presentIds.has(storedInstanceId)) return storedInstanceId;
  if (servingInstanceId && presentIds.has(servingInstanceId)) return servingInstanceId;
  if (daemons.length > 0) return daemons[0].instanceId;
  return null;
}

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

interface SelectedDaemonContextValue {
  readonly room: Room | null;
  readonly daemons: DaemonHost[];
  readonly selectedInstanceId: string | null;
  readonly servingInstanceId?: string;
  readonly selectDaemon: (instanceId: string) => void;
}

const SelectedDaemonContext = createContext<SelectedDaemonContextValue | null>(null);

export interface SelectedDaemonProviderProps {
  livekitUrl?: string;
  commonRoom?: string;
  /** The instance id of the daemon that served this web bundle (`/api/config`'s `daemon_instance_id`). */
  servingInstanceId?: string;
  /**
   * Test-injection seam (mirrors `RpcTransportProviderProps.liveKitFactory`): when provided, used
   * directly instead of joining the common room via `useCommonRoom`. No production caller sets this.
   */
  room?: Room | null;
  /**
   * Test-injection seam (mirrors `RpcTransportProviderProps.liveKitFactory`): when provided, used
   * directly instead of deriving daemons from `useRoomParticipants` + `daemonHostsFromParticipants`.
   * No production caller sets this.
   */
  daemons?: DaemonHost[];
  children: ReactNode;
}

/**
 * Resolve `{ room, daemons }` for the provider: the test-injection overrides when given, otherwise
 * the production path — join the common room as this user's presence identity, then derive the
 * daemon list from its participants.
 */
function useCommonRoomDaemons(
  livekitUrl: string | undefined,
  commonRoom: string | undefined,
  roomOverride: Room | null | undefined,
  daemonsOverride: DaemonHost[] | undefined,
): { room: Room | null; daemons: DaemonHost[] } {
  // TODO: migrate to `useAuthContext()` once every `withSelectedDaemon`-based test provides an
  // `AuthProvider` ancestor. Left on the standalone `useAuth()` hook deliberately for now: this
  // component is mounted once for the whole daemon-mode session (it wraps, and is never remounted
  // by, the `key={selectedInstanceId}` boundary below), so it isn't subject to the remount-destroys-
  // the-refresh-timer bug that motivated `AuthProvider` — it only needs `user`/`isAuthenticated` to
  // derive a LiveKit presence identity, not a coordinated session token. Migrating it purely for
  // consistency would force every `withSelectedDaemon`/`SelectedDaemonProvider`-based Cypress test
  // across the suite to add an `AuthProvider` wrapper, which is out of scope for this fix.
  const { user, isAuthenticated } = useAuth();
  const identity = useMemo(
    () => (user ? presenceIdentityForUser(user.login) : undefined),
    [user],
  );
  const { room: producedRoom } = useCommonRoom(
    livekitUrl,
    commonRoom,
    isAuthenticated ? identity : undefined,
  );
  const room = roomOverride !== undefined ? roomOverride : producedRoom;

  const participants = useRoomParticipants(daemonsOverride !== undefined ? null : room);
  const derivedDaemons = useMemo(() => daemonHostsFromParticipants(participants), [participants]);
  const daemons = daemonsOverride !== undefined ? daemonsOverride : derivedDaemons;

  return { room, daemons };
}

/**
 * Own the currently selected daemon's state: initialized from {@link resolveSelectedDaemonInstanceId},
 * recomputed whenever `daemons` changes (so a selection whose daemon left the common room falls
 * back to the serving daemon / first available daemon instead of pointing at a dead peer), and
 * persisted to `sessionStorage` on explicit selection.
 *
 * An empty `daemons` list is treated as "no information yet" rather than "no daemons exist" and
 * never clears an existing selection: the common room's connection is not always up (the initial
 * connect, or a transient disconnect/reconnect — see `useCommonRoom`) and `daemons` is briefly
 * empty in both cases. Resetting the selection during that gap would flash the UI to "nothing
 * selected" and null out every `useDaemonClient` consumer's RPC client, even though the daemon is
 * still there and about to reappear.
 */
function useSelectedDaemonState(
  daemons: DaemonHost[],
  servingInstanceId: string | undefined,
): { selectedInstanceId: string | null; selectDaemon: (instanceId: string) => void } {
  const [selectedInstanceId, setSelectedInstanceId] = useState<string | null>(() =>
    resolveSelectedDaemonInstanceId({
      daemons,
      servingInstanceId,
      storedInstanceId: readStoredSelectedDaemon(),
    }),
  );

  useEffect(() => {
    if (daemons.length === 0) return;
    setSelectedInstanceId((current) =>
      resolveSelectedDaemonInstanceId({ daemons, servingInstanceId, storedInstanceId: current }),
    );
  }, [daemons, servingInstanceId]);

  const selectDaemon = useCallback((instanceId: string) => {
    writeStoredSelectedDaemon(instanceId);
    setSelectedInstanceId(instanceId);
  }, []);

  return { selectedInstanceId, selectDaemon };
}

/**
 * Provide the shared common-room connection, the daemon list, and the currently selected daemon to
 * the component subtree. Mount once around the daemon-mode screen dispatch (see `index.tsx`).
 */
export function SelectedDaemonProvider({
  livekitUrl,
  commonRoom,
  servingInstanceId,
  room: roomOverride,
  daemons: daemonsOverride,
  children,
}: SelectedDaemonProviderProps) {
  const { room, daemons } = useCommonRoomDaemons(livekitUrl, commonRoom, roomOverride, daemonsOverride);
  const { selectedInstanceId, selectDaemon } = useSelectedDaemonState(daemons, servingInstanceId);

  const value: SelectedDaemonContextValue = useMemo(
    () => ({ room, daemons, selectedInstanceId, servingInstanceId, selectDaemon }),
    [room, daemons, selectedInstanceId, servingInstanceId, selectDaemon],
  );

  // Give the screen subtree a fresh lifecycle whenever the selected daemon changes: keying the
  // children by `selectedInstanceId` remounts them, so each daemon-mode screen resets its transient
  // state (selected session, open inspector, live terminal attachment, create/VM/task UI) and
  // re-runs its data fetches against the newly selected daemon — a full reload, not just a refetch.
  // The provider itself stays mounted above the key, so the shared common-room connection persists.
  return (
    <SelectedDaemonContext.Provider value={value}>
      <Fragment key={selectedInstanceId ?? "__no-daemon__"}>{children}</Fragment>
    </SelectedDaemonContext.Provider>
  );
}

// ---------------------------------------------------------------------------
// Hooks
// ---------------------------------------------------------------------------

const NO_PROVIDER_DEFAULTS: SelectedDaemonContextValue = {
  room: null,
  daemons: [],
  selectedInstanceId: null,
  servingInstanceId: undefined,
  selectDaemon: () => {},
};

/**
 * Return the shared daemon-selection context. Mirrors `useHttpTransport`/`useLiveKitTransportFactory`
 * (`./transportProvider`): when no `SelectedDaemonProvider` wraps this component, sensible empty
 * defaults are returned rather than throwing.
 */
export function useSelectedDaemon(): SelectedDaemonContextValue {
  return useContext(SelectedDaemonContext) ?? NO_PROVIDER_DEFAULTS;
}

/** Convenience for `useSelectedDaemon().daemons`. */
export function useDaemons(): DaemonHost[] {
  return useSelectedDaemon().daemons;
}

/**
 * Build and memoize a ConnectRPC client for a daemon-level service, targeting a specific daemon's
 * RPC-server identity (`daemon-{instanceId}`) over the shared common-room LiveKit connection.
 * Returns `null` until the room is connected and `instanceId` is set — callers must guard call
 * sites. Use this to address a daemon other than the currently selected one (e.g. adding a project
 * to a chosen host); {@link useDaemonClient} is the selected-daemon convenience over it.
 */
export function useDaemonClientFor<S extends DescService>(
  service: S,
  instanceId: string | null,
): Client<S> | null {
  const { room } = useSelectedDaemon();
  return useLiveKitClient(service, room, instanceId ? daemonRpcIdentity(instanceId) : null);
}

/**
 * Build and memoize a ConnectRPC client for a daemon-level service, targeting the currently
 * selected daemon's RPC-server identity over the shared common-room LiveKit connection. Returns
 * `null` until a daemon is selected and the room is connected — callers must guard call sites.
 */
export function useDaemonClient<S extends DescService>(service: S): Client<S> | null {
  const { selectedInstanceId } = useSelectedDaemon();
  return useDaemonClientFor(service, selectedInstanceId);
}
