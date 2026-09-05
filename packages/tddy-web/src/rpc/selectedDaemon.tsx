/**
 * Shared common-room daemon-selection context.
 *
 * A `tddy-daemon` joins the common room as two participants (see `participantRole.ts`'s
 * `daemonRpcIdentity` doc comment): the selector lists daemons by their discovery identity, but
 * daemon-level RPC (`ConnectionService`, `TaskService`, `VmService`, …) must address
 * `daemon-{instanceId}`. `SelectedDaemonProvider` owns the one common-room connection shared by
 * every daemon-mode screen, the currently selected daemon, and `useDaemonClient` — the daemon-level
 * equivalent of `useHttpClient` from `./transportProvider`.
 *
 * *Which* daemons exist is no longer this module's answer. It composes the host directory's sources
 * (`./hostDirectory`) — the common room, and the daemon that served the page — and reads the merged
 * list off `useHostDirectory`, so a build with no common room still has a host to offer. The room
 * itself is offered to the subtree as a *connection provider* (`./connections/liveKit`), so a screen
 * asks for a host rather than for a room and a participant identity, and `useDaemonClient` is
 * `useHostClient` under a daemon-shaped name. The room is **not** on this context any more: a screen
 * that wants the participant roster asks `useHostPresence` for it, and can be refused.
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
import type { DaemonHost } from "../lib/participantRole";
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
import { LiveKitConnections } from "./connections/liveKit";
import { useHostClient } from "./connections/registry";
import type { ConnectionStatus } from "./connections/types";
import { daemonHostOf } from "./hostDirectory/daemonHost";
import { useLiveKitHostDirectorySource } from "./hostDirectory/liveKitSource";
import { HostPresenceRoom } from "./hostDirectory/presenceRoom";
import { useServingHostDirectorySource } from "./hostDirectory/servingSource";
import { HostDirectorySources, useHostDirectory } from "./hostDirectory/useHostDirectory";
import type { HostDirectorySource } from "./hostDirectory/types";

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
  readonly daemons: DaemonHost[];
  readonly selectedInstanceId: string | null;
  readonly servingInstanceId?: string;
  readonly selectDaemon: (instanceId: string) => void;
  /**
   * How the host directory is doing — whether this page can name the hosts it could talk to at all.
   *
   * An empty `daemons` alone cannot answer that: it means both "the directory is up and this fleet
   * has no daemons" and "nothing has told us yet", and the confusion is what left the presence panel
   * claiming it was connecting for as long as the tab stayed open. Optimistic across sources (see
   * `hostDirectory/useHostDirectory`): one source that can name hosts is a usable directory, so an
   * unreachable common room does not make a page that knows its own daemon look broken. To ask about
   * one source in particular — as the LiveKit presence screen does — read
   * `useHostDirectory().sources`.
   */
  readonly directoryStatus: ConnectionStatus;
  /**
   * Why the directory is unusable, when {@link directoryStatus} is `"error"`; `null` otherwise. A
   * failure on one source while another still names hosts is that source's, and is read off
   * `useHostDirectory().sources`.
   */
  readonly directoryError: string | null;
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
   * Test-injection seam (mirrors `RpcTransportProviderProps.liveKitFactory`): when provided, it is
   * the common room's contribution to the directory, used instead of deriving one from
   * `useRoomParticipants` + `daemonHostsFromParticipants`. The serving host is still contributed
   * alongside it, exactly as in production. No production caller sets this.
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
 * Assemble the directory's sources for the provider, and hand back the room they were assembled
 * over.
 *
 * The room comes back because two things above the directory still need the object — the connection
 * provider that reaches hosts over it, and the presence context a roster-reading screen asks
 * through. Neither is the directory's business, which is why nothing publishes it.
 *
 * Order is precedence, and the common room comes first deliberately: it carries a daemon's own
 * advertisement, with the label it chose for itself, its `repos_base_path` and its
 * `max_attachment_bytes`. The serving source knows an instance id and nothing else, so letting it
 * win over the room's account of the same machine would silently drop the attachment cap the
 * Start-Session form enforces. It contributes the serving daemon when the room did not — which is
 * every case where there is no room, and the whole point of the exercise.
 */
function useDirectorySources({
  livekitUrl,
  commonRoom,
  servingInstanceId,
  room: roomOverride,
  daemons: daemonsOverride,
  roomFactory,
}: Omit<SelectedDaemonProviderProps, "children">): {
  sources: readonly HostDirectorySource[];
  room: Room | null;
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
  const { source: liveKitSource, room } = useLiveKitHostDirectorySource({
    livekitUrl,
    commonRoom,
    identity: isAuthenticated ? identity : undefined,
    room: roomOverride,
    hosts: daemonsOverride,
    roomFactory,
  });
  const servingSource = useServingHostDirectorySource(servingInstanceId);
  const sources = useMemo(
    () => [liveKitSource, servingSource],
    [liveKitSource, servingSource],
  );
  return { sources, room };
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
 * Provide the host directory, the currently selected daemon, and the wires that reach hosts to the
 * component subtree. Mount once around the daemon-mode screen dispatch (see `index.tsx`).
 *
 * The three contexts nest in dependency order, all of them above the remount key: the directory's
 * sources, the presence room they were assembled over, and the connection provider that reaches
 * hosts over it.
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
  const { sources, room } = useDirectorySources({
    livekitUrl,
    commonRoom,
    servingInstanceId,
    room: roomOverride,
    daemons: daemonsOverride,
    roomFactory,
  });

  return (
    <HostDirectorySources sources={sources}>
      <HostPresenceRoom room={room}>
        <LiveKitConnections room={room}>
          <SelectedDaemonScope servingInstanceId={servingInstanceId}>{children}</SelectedDaemonScope>
        </LiveKitConnections>
      </HostPresenceRoom>
    </HostDirectorySources>
  );
}

/**
 * Read the merged directory and own the selection over it.
 *
 * A component of its own rather than part of {@link SelectedDaemonProvider} so that it reads the
 * directory the same way every other consumer does — through `useHostDirectory` — instead of the
 * provider quietly holding a merge nobody else can see.
 */
function SelectedDaemonScope({
  servingInstanceId,
  children,
}: {
  servingInstanceId: string | undefined;
  children: ReactNode;
}) {
  const directory = useHostDirectory();
  // The screens speak `DaemonHost`; the directory speaks `HostDescriptor`. See `hostDirectory/daemonHost`.
  const daemons = useMemo(() => directory.hosts.map(daemonHostOf), [directory.hosts]);
  const { selectedInstanceId, selectDaemon } = useSelectedDaemonState(daemons, servingInstanceId);

  const value: SelectedDaemonContextValue = useMemo(
    () => ({
      daemons,
      selectedInstanceId,
      servingInstanceId,
      selectDaemon,
      directoryStatus: directory.status,
      directoryError: directory.error,
    }),
    [
      daemons,
      selectedInstanceId,
      servingInstanceId,
      selectDaemon,
      directory.status,
      directory.error,
    ],
  );

  // Give the screen subtree a fresh lifecycle whenever the selected daemon changes: keying the
  // children by `selectedInstanceId` remounts them, so each daemon-mode screen resets its transient
  // state (selected session, open inspector, live terminal attachment, create/VM/task UI) and
  // re-runs its data fetches against the newly selected daemon — a full reload, not just a refetch.
  // The provider itself stays mounted above the key, so the shared common-room connection persists.
  //
  // `LiveKitConnections` — which offers that connection to the subtree as a wire hosts are resolved
  // over — sits above the key for the same reason: a host change reloads the screens, it does not
  // re-register the wire that reaches them.
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
  daemons: [],
  selectedInstanceId: null,
  servingInstanceId: undefined,
  selectDaemon: () => {},
  directoryStatus: "idle",
  directoryError: null,
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
 * Build and memoize a ConnectRPC client for a daemon-level service, targeting a specific daemon over
 * whichever wire can reach it — the common room today, an in-process bridge in a host build that
 * registers one. Returns `null` until a provider can reach `instanceId` and `instanceId` is set —
 * callers must guard call sites. Use this to address a daemon other than the currently selected one
 * (e.g. adding a project to a chosen host); {@link useDaemonClient} is the selected-daemon
 * convenience over it.
 *
 * A thin name over {@link useHostClient}: a daemon *is* a host, and this hook is kept because the
 * screens read in that vocabulary.
 */
export function useDaemonClientFor<S extends DescService>(
  service: S,
  instanceId: string | null,
): Client<S> | null {
  return useHostClient(service, instanceId);
}

/**
 * Build and memoize a ConnectRPC client for a daemon-level service, targeting the currently
 * selected daemon. Returns `null` until a daemon is selected and a provider can reach it — callers
 * must guard call sites.
 */
export function useDaemonClient<S extends DescService>(service: S): Client<S> | null {
  const { selectedInstanceId } = useSelectedDaemon();
  return useDaemonClientFor(service, selectedInstanceId);
}
