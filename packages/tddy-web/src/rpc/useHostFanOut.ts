/**
 * The cross-daemon read every fleet-wide list in this app performs.
 *
 * A daemon answers a list RPC for **itself**: the requests carry no routing field and a daemon never
 * forwards one to its peers. So anything that has to show the whole fleet — the agents a session can
 * be started as, the agents it can attach, the model registry — has to ask every daemon itself, and
 * every one of those surfaces needs the same three properties:
 *
 *   • **one client per host**, each peer addressed at its `daemon-{instanceId}` RPC identity over the
 *     shared common-room connection — the identity a daemon actually serves RPC on;
 *   • **isolated reads**, so one unreachable host costs one error row and never the list;
 *   • **one row per identity**, because two hosts can name the same row — a def created on one host
 *     and known to another is one agent, and must be offered once.
 *
 * This hook owns those three and nothing else. What service to talk to, what a row is and what makes
 * two rows the same row are the caller's, passed in as a {@link HostReader}.
 *
 * `components/models/useModelRegistryFanOut` fans out the same way but is not expressible here: it
 * reads several RPCs per host into one composite per-host snapshot, merges them by provider identity
 * rather than by row key, and issues writes back to the owning host. It shares the shape, not the
 * contract.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ConnectError, type Transport } from "@connectrpc/connect";
import { daemonRpcIdentity } from "../lib/participantRole";
import { useSelectedDaemon } from "./selectedDaemon";
import { useLiveKitTransportFactory } from "./transportProvider";

/** A host that could not be asked, or refused to answer. Rendered as one row, never swallowed. */
export interface HostReadFailure {
  readonly daemonInstanceId: string;
  readonly message: string;
}

export interface HostFanOut<T> {
  /** Every host's rows, in the order the hosts are read, de-duplicated by the reader's key. */
  readonly rows: T[];
  readonly failures: HostReadFailure[];
}

/**
 * What to ask each host, and what its answers are.
 *
 * Declare one at module scope. It describes a service, not state, so a value that changes between
 * renders would only mean the same fan-out re-described — see {@link useHostFanOut} on how a new one
 * is treated.
 */
export interface HostReader<C, T> {
  /** A client for the daemon reached over `transport`, i.e. which service the read is issued against. */
  readonly clientFor: (transport: Transport) => C;
  /**
   * One host's rows. `daemonInstanceId` names the host being read, for readers whose rows carry the
   * host they came from and whose wire format does not.
   */
  readonly read: (
    client: C,
    daemonInstanceId: string,
    signal: AbortSignal,
  ) => Promise<readonly T[]>;
  /** What makes two rows the same row across hosts. Rows repeating a key are dropped, first wins. */
  readonly keyOf: (row: T) => string;
}

/** One host's answer: the rows it offers, or why it could not say. */
interface HostAnswer<T> {
  readonly rows: readonly T[];
  /**
   * Non-empty when the read failed. Held apart from empty `rows` because the two claims are
   * different — "this host offers nothing" versus "this host did not answer" — and only the first one
   * may be rendered as an absence.
   */
  readonly error: string;
}

/** What a caller is told when the common room holds no connection to the host it addressed. */
export function noConnectionTo(daemonInstanceId: string): string {
  return `no connection to daemon ${daemonInstanceId}`;
}

/**
 * Read `reader`'s rows from the daemon `homeClient` addresses and from every other daemon in the
 * common room.
 *
 * `homeClient` is passed in rather than derived because the surface that owns it already has one: a
 * form is handed the selected daemon's client, and a session pane talks over the shared transport to
 * the daemon facilitating its session. `homeInstanceId` names the host behind it, both to attribute
 * its rows and so its failure gets an error row naming a host an operator can go and look at.
 *
 * An **unnamed** home host (`homeInstanceId === ""`) is therefore not read through `homeClient` while
 * the common room advertises hosts: that read could attribute nothing and its failure could name no
 * host to go and look at, and the host behind it is already in the advertised list under an id the
 * room *can* name. Reading it there instead of through `homeClient` costs no host — every advertised
 * one is still read, at its own identity, exactly once — where reading both spellings asked one
 * daemon twice and could report a healthy host as unreachable off the second answer. With no host
 * advertised there is nothing to name and nothing to disambiguate, so `homeClient` is the read.
 *
 * `reader` is held in a ref: the reads are restarted by the host list changing, never by the caller
 * re-describing the same service, so an inline reader cannot turn every render into a fresh round of
 * RPCs. A reader swapped for a different one therefore takes effect on the next read rather than
 * immediately — which is why it belongs at module scope.
 */
export function useHostFanOut<C, T>(
  homeClient: C,
  homeInstanceId: string,
  reader: HostReader<C, T>,
): HostFanOut<T> {
  const { room, daemons } = useSelectedDaemon();
  const liveKitFactory = useLiveKitTransportFactory();
  const [answers, setAnswers] = useState<ReadonlyMap<string, HostAnswer<T>>>(new Map());

  const readerRef = useRef(reader);
  readerRef.current = reader;

  // A read that lands after the surface is gone has nobody to render it, and writing it would be a
  // state update against a dead component.
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const readHost = useCallback(
    async (instanceId: string, client: C, signal: AbortSignal): Promise<void> => {
      const put = (answer: HostAnswer<T>) => {
        if (!mounted.current || signal.aborted) return;
        setAnswers((current) => new Map(current).set(instanceId, answer));
      };
      try {
        put({ rows: await readerRef.current.read(client, instanceId, signal), error: "" });
      } catch (err) {
        put({ rows: [], error: ConnectError.from(err).rawMessage });
      }
    },
    [],
  );

  // The daemon list is rebuilt on every common-room participant event, so its array identity changes
  // far more often than its contents; depending on the ids keeps the fan-out to one read per host
  // per actual change. `\n` cannot occur in a daemon instance id (they are LiveKit participant
  // identity segments).
  const peerIdsKey = daemons
    .map((d) => d.instanceId)
    .filter((id) => id !== "" && id !== homeInstanceId)
    .join("\n");
  const peerIds = useMemo(() => peerIdsKey.split("\n").filter((id) => id !== ""), [peerIdsKey]);

  // Which hosts this fan-out is answering for, in the order they are rendered. `homeClient` is one
  // of them only when it names its host, or when no host is advertised for it to be read as; see the
  // doc comment. Deriving the effect and the assembly below from one list is what keeps an answer
  // recorded before the room advertised anything from being rendered afterwards, when the same host
  // is being read under its own name as well.
  const readsHomeClient = homeInstanceId !== "" || peerIds.length === 0;
  const hostsRead = useMemo(
    () => (readsHomeClient ? [homeInstanceId, ...peerIds] : peerIds),
    [readsHomeClient, homeInstanceId, peerIds],
  );

  useEffect(() => {
    const reads = new AbortController();
    if (readsHomeClient) void readHost(homeInstanceId, homeClient, reads.signal);
    for (const instanceId of peerIds) {
      // Every peer is addressed over the shared common-room connection; without a room there is no
      // way to reach it, and saying so beats a list that quietly omits that host.
      if (!room) {
        setAnswers((current) =>
          new Map(current).set(instanceId, { rows: [], error: noConnectionTo(instanceId) }),
        );
        continue;
      }
      const transport = liveKitFactory(room, daemonRpcIdentity(instanceId));
      void readHost(instanceId, readerRef.current.clientFor(transport), reads.signal);
    }
    // Unmounting — or moving on to a different host list — cancels the reads in flight, so no answer
    // to a question nobody is asking any more is waited for or written.
    return () => reads.abort();
  }, [homeClient, homeInstanceId, readsHomeClient, peerIds, room, liveKitFactory, readHost]);

  return useMemo(() => {
    const rows: T[] = [];
    const failures: HostReadFailure[] = [];
    const seen = new Set<string>();
    for (const instanceId of hostsRead) {
      const answer = answers.get(instanceId);
      if (!answer) continue;
      if (answer.error !== "") {
        failures.push({ daemonInstanceId: instanceId, message: answer.error });
        continue;
      }
      for (const row of answer.rows) {
        const key = readerRef.current.keyOf(row);
        if (seen.has(key)) continue;
        seen.add(key);
        rows.push(row);
      }
    }
    return { rows, failures };
  }, [answers, hostsRead]);
}
