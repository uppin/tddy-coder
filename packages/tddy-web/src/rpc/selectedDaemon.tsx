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
import {
  useCommonRoom,
  useObservedCommonRoomStatus,
  type CommonRoomStatus,
} from "../hooks/useCommonRoom";
import { useRoomParticipants } from "../hooks/useRoomParticipants";
import { daemonHostsFromParticipants, daemonRpcIdentity, type DaemonHost } from "../lib/participantRole";
import { presenceIdentityForUser } from "../lib/presenceIdentity";
import {
  readStoredSelectedDaemon,
  resolveSelectedDaemonInstanceId,
  writeStoredSelectedDaemon,
} from "../routing/selectedHost";
import { PARAM_HOST, screenRootOf, withParams } from "../routing/appLocation";
import {
  navigateAppLocation,
  readAppLocation,
  setAppLocationParams,
  useAppLocation,
} from "../routing/useAppLocation";
import { useLiveKitClient } from "./transportProvider";

// ---------------------------------------------------------------------------
// Persistence + resolution
// ---------------------------------------------------------------------------

// The rules themselves are pure and live in `routing/selectedHost` so they are unit-testable
// without React's JSX runtime; re-exported here for this module's existing importers.
export {
  readStoredSelectedDaemon,
  writeStoredSelectedDaemon,
  resolveSelectedDaemonInstanceId,
} from "../routing/selectedHost";

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

interface SelectedDaemonContextValue {
  readonly room: Room | null;
  readonly daemons: DaemonHost[];
  readonly selectedInstanceId: string | null;
  readonly servingInstanceId?: string;
  readonly selectDaemon: (instanceId: string) => void;
  /**
   * How the shared common-room connection is doing. `room` alone cannot answer that: it is `null`
   * both while the join is in flight and after it failed, so a screen holding only the room cannot
   * tell "still connecting" from "cannot connect" — the confusion that left the presence panel
   * claiming it was connecting for as long as the tab stayed open.
   */
  readonly roomStatus: CommonRoomStatus;
  /** Why the common room is unusable, when {@link roomStatus} is `"error"`; `null` otherwise. */
  readonly roomError: string | null;
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
  /**
   * Test-injection seam (mirrors `RpcTransportProviderProps.liveKitFactory`): the `Room` object
   * `useCommonRoom` joins with. Unlike `room`/`daemons`, which are *result* seams that skip the
   * join entirely, this one leaves the provider on its production path — authenticate, mint a
   * LiveKit token, `connect()` — so a test can drive a join that fails or never settles. No
   * production caller sets this.
   */
  roomFactory?: () => Room;
  children: ReactNode;
}

/**
 * Resolve `{ room, daemons, roomStatus, roomError }` for the provider: the test-injection overrides
 * when given, otherwise the production path — join the common room as this user's presence
 * identity, then derive the daemon list from its participants.
 *
 * The published status has one rule, which covers the `room` override without special-casing it:
 * until there is a room object, it is the outcome of the join attempt (`useCommonRoom`); once
 * there is one, it is that room's own live connection state, so a drop after a successful join is
 * reported too. An injected room therefore speaks for itself — a room double standing in for a
 * joined room reports `ConnectionState.Connected` and is published as `"connected"`.
 */
function useCommonRoomDaemons(
  livekitUrl: string | undefined,
  commonRoom: string | undefined,
  roomOverride: Room | null | undefined,
  daemonsOverride: DaemonHost[] | undefined,
  roomFactory: (() => Room) | undefined,
): {
  room: Room | null;
  daemons: DaemonHost[];
  roomStatus: CommonRoomStatus;
  roomError: string | null;
} {
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
  const {
    room: producedRoom,
    status: joinStatus,
    error: joinError,
  } = useCommonRoom(livekitUrl, commonRoom, isAuthenticated ? identity : undefined, roomFactory);
  const room = roomOverride !== undefined ? roomOverride : producedRoom;

  const participants = useRoomParticipants(daemonsOverride !== undefined ? null : room);
  const derivedDaemons = useMemo(() => daemonHostsFromParticipants(participants), [participants]);
  const daemons = daemonsOverride !== undefined ? daemonsOverride : derivedDaemons;

  const observed = useObservedCommonRoomStatus(room);

  return {
    room,
    daemons,
    roomStatus: room ? observed.status : joinStatus,
    roomError: room ? observed.error : joinError,
  };
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
  const { location } = useAppLocation();
  const urlInstanceId = location.params[PARAM_HOST] ?? null;

  const [selectedInstanceId, setSelectedInstanceId] = useState<string | null>(() =>
    resolveSelectedDaemonInstanceId({
      daemons,
      servingInstanceId,
      storedInstanceId: readStoredSelectedDaemon(),
      urlInstanceId,
    }),
  );

  useEffect(() => {
    if (daemons.length === 0) return;
    setSelectedInstanceId((current) =>
      resolveSelectedDaemonInstanceId({
        daemons,
        servingInstanceId,
        storedInstanceId: current,
        urlInstanceId,
      }),
    );
  }, [daemons, servingInstanceId, urlInstanceId]);

  // Record the resolved host so the very first reload — or a copy of the address bar into another
  // tab, which carries no `sessionStorage` — already names it. `replace`: nobody chose this, it is
  // the default made explicit.
  useEffect(() => {
    if (!selectedInstanceId || urlInstanceId === selectedInstanceId) return;
    setAppLocationParams({ [PARAM_HOST]: selectedInstanceId }, { replace: true });
  }, [selectedInstanceId, urlInstanceId]);

  /**
   * An explicit host change is a navigation: it pushes a history entry, and it drops the
   * sub-selection, since a session id from the old host names nothing on the new one (the
   * `key={selectedInstanceId}` remount below invalidates it either way).
   */
  const selectDaemon = useCallback((instanceId: string) => {
    writeStoredSelectedDaemon(instanceId);
    const current = readAppLocation();
    navigateAppLocation(
      withParams({ path: screenRootOf(current.path), params: current.params }, {
        [PARAM_HOST]: instanceId,
      }),
    );
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
  roomFactory,
  children,
}: SelectedDaemonProviderProps) {
  const { room, daemons, roomStatus, roomError } = useCommonRoomDaemons(
    livekitUrl,
    commonRoom,
    roomOverride,
    daemonsOverride,
    roomFactory,
  );
  const { selectedInstanceId, selectDaemon } = useSelectedDaemonState(daemons, servingInstanceId);

  const value: SelectedDaemonContextValue = useMemo(
    () => ({
      room,
      daemons,
      selectedInstanceId,
      servingInstanceId,
      selectDaemon,
      roomStatus,
      roomError,
    }),
    [room, daemons, selectedInstanceId, servingInstanceId, selectDaemon, roomStatus, roomError],
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
  roomStatus: "idle",
  roomError: null,
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
