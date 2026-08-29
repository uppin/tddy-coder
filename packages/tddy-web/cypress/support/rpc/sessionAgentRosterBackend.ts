/**
 * In-memory backends for the session agent roster (docs/ft/daemon/session-agent-roster.md).
 *
 * Two things are modelled here that a plain `.onUnary` stub cannot express:
 *
 *  - `StreamSessionAgents` is snapshot-then-changes and stays open, so the roster is served from a
 *    pushable tail — a test attaches an agent "from another browser tab" by calling `pushRoster`,
 *    and the pane must update without a remount. Modelled on `liveKitRoomsBackend.ts`, which has
 *    the same shape.
 *  - The agent picker fans out across daemons, so `ListSubagents` is answered by a *different*
 *    backend per host. An option appearing under host B is therefore proof the fan-out reached B,
 *    not proof of a fixture.
 */

import { Code, ConnectError, type ServiceImpl } from "@connectrpc/connect";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  AgentCloneState,
  ConnectionService,
  ListSubagentsResponseSchema,
  SessionAgentRosterSchema,
  type SessionAgentEntry,
} from "../../../src/gen/connection_pb";

/** A roster entry with sensible defaults; tests override only what the scenario is about. */
export function anAttachedAgent(
  agentId: string,
  overrides: Partial<SessionAgentEntry> = {},
): SessionAgentEntry {
  const [name, daemonInstanceId] = agentId.split("@");
  return {
    agentId,
    name,
    daemonInstanceId,
    label: `${name} (${daemonInstanceId})`,
    model: "qwen2.5-coder:7b",
    replaces: [],
    tools: ["Read", "Glob", "Grep"],
    codebaseSessionId: "",
    cloneState: AgentCloneState.LOCAL,
    cloneError: "",
    ...overrides,
  } as SessionAgentEntry;
}

/** An agent served by a remote daemon, with a clone behind it. */
export function aRemoteAttachedAgent(
  agentId: string,
  overrides: Partial<SessionAgentEntry> = {},
): SessionAgentEntry {
  return anAttachedAgent(agentId, {
    codebaseSessionId: "1780828020298-clone",
    cloneState: AgentCloneState.READY,
    ...overrides,
  });
}

/** One agent a daemon offers, as `ListSubagents` returns it. */
export function anAvailableAgent(
  name: string,
  daemonInstanceId: string,
  replaces: string[] = [],
) {
  return {
    name,
    label: `${name} (${daemonInstanceId})`,
    model: "qwen2.5-coder:7b",
    daemonInstanceId,
    agentId: `${name}@${daemonInstanceId}`,
    replaces,
    tools: ["Read", "Glob", "Grep"],
  };
}

export interface RosterScenario {
  sessionId: string;
  /** The roster the first frame carries. */
  initial: SessionAgentEntry[];
  /** The revision the first frame carries. Defaults to `initial.length`. */
  rev?: number;
  /** When set, the stream fails before ever producing a snapshot — the "read failed" state. */
  failBeforeSnapshot?: string;
  /** When set, the stream never produces a frame — the "loading" state. */
  neverAnswers?: boolean;
  /**
   * What this daemon offers when the picker asks it — this host's own answer, never a peer's. A
   * spec whose subject *is* the fan-out gives every other host its own backend
   * (`aDaemonOfferingAgents` / `aDaemonThatCannotBeReached` behind `mountWithPerDaemonLiveKitRpc`),
   * so an option or an error row attributed to that host is proof of routing, not of a fixture.
   */
  offers?: ReturnType<typeof anAvailableAgent>[];
  /**
   * When set, this daemon serves the roster but cannot answer the picker's fan-out — the case where
   * the host holding the browser's own transport is the one that failed, and the error row must name
   * it rather than whichever host the session belongs to.
   */
  offersUnavailable?: string;
}

/** Which session, on which daemon, a roster call named — the routing half of a request. */
export interface RosterAddress {
  sessionId: string;
  daemonInstanceId: string;
}

/** The controls a spec drives the roster fake with, whichever backend serves it. */
export interface RosterControls {
  /** Publish a new roster revision to every open stream, as a daemon does on attach/detach. */
  pushRoster: (agents: SessionAgentEntry[], rev: number) => void;
  /** Qualified ids passed to `DetachSessionAgent`, in call order. */
  detachedAgentIds: () => string[];
  /** Qualified ids passed to `AttachSessionAgent`, in call order. */
  attachedAgentIds: () => string[];
  /**
   * What `StreamSessionAgents` named, in call order. A split session keeps its roster on the
   * codebase host under the codebase half's session id, so which pair a caller sent is the whole
   * question of whether it read the roster that governs the session or an empty one beside it.
   */
  rosterReadsAddressed: () => RosterAddress[];
  /** What `AttachSessionAgent` named, in call order — the same question, for a write. */
  attachesAddressed: () => RosterAddress[];
}

export interface RosterBackend extends RosterControls {
  backend: InMemoryRpcBackend;
}

/** The roster fake as handlers, so a screen's own backend can serve it too. */
export interface SessionAgentRosterFake extends RosterControls {
  /**
   * The `ConnectionService` methods that serve the roster, to be spread into the **one**
   * `.implement(ConnectionService, …)` call a backend makes: Connect's router fills every method a
   * service implementation omits with an `Unimplemented` handler, so a second registration of the
   * same service would shadow the first one's methods.
   */
  handlers: Partial<ServiceImpl<typeof ConnectionService>>;
}

export function aSessionAgentRosterFake(scenario: RosterScenario): SessionAgentRosterFake {
  const tail = aRosterTail();
  const attached: string[] = [];
  const detached: string[] = [];
  const rosterReads: RosterAddress[] = [];
  const attaches: RosterAddress[] = [];

  const handlers: Partial<ServiceImpl<typeof ConnectionService>> = {
    async listSubagents() {
      if (scenario.offersUnavailable !== undefined) {
        throw new ConnectError(scenario.offersUnavailable, Code.Unavailable);
      }
      return create(ListSubagentsResponseSchema, { subagents: scenario.offers ?? [] });
    },
    async *streamSessionAgents(req) {
      rosterReads.push({ sessionId: req.sessionId, daemonInstanceId: req.daemonInstanceId });
      if (scenario.failBeforeSnapshot !== undefined) {
        throw new ConnectError(scenario.failBeforeSnapshot, Code.Unavailable);
      }
      if (scenario.neverAnswers === true) {
        // Never yields and never returns — the pane must show "loading", not "empty".
        await new Promise<never>(() => {});
      }
      yield create(SessionAgentRosterSchema, {
        sessionId: scenario.sessionId,
        rev: BigInt(scenario.rev ?? scenario.initial.length),
        agents: scenario.initial,
      });
      yield* tail.frames();
    },
    // A daemon that changes the roster publishes the new revision to every open stream — including
    // the stream held by the browser that asked for the change. Pushing here rather than only
    // answering is what makes a pane that fires the call and never re-renders observable.
    async attachSessionAgent(req) {
      attached.push(req.agentId);
      attaches.push({ sessionId: req.sessionId, daemonInstanceId: req.daemonInstanceId });
      const agents = [...tail.currentAgents(), attachedEntryFor(req.agentId, scenario.offers ?? [])];
      tail.push(agents, tail.currentRev() + 1);
      return create(SessionAgentRosterSchema, {
        sessionId: scenario.sessionId,
        rev: BigInt(tail.currentRev()),
        agents: tail.currentAgents(),
      });
    },
    async detachSessionAgent(req) {
      detached.push(req.agentId);
      const agents = tail.currentAgents().filter((a) => a.agentId !== req.agentId);
      tail.push(agents, tail.currentRev() + 1);
      return create(SessionAgentRosterSchema, {
        sessionId: scenario.sessionId,
        rev: BigInt(tail.currentRev()),
        agents: tail.currentAgents(),
      });
    },
  };

  tail.seed(scenario.initial, scenario.rev ?? scenario.initial.length);

  return {
    handlers,
    pushRoster: (agents, rev) => tail.push(agents, rev),
    detachedAgentIds: () => [...detached],
    attachedAgentIds: () => [...attached],
    rosterReadsAddressed: () => [...rosterReads],
    attachesAddressed: () => [...attaches],
  };
}

/** The roster fake on a backend of its own — all a spec mounting the pane alone needs. */
export function aSessionAgentRosterBackend(scenario: RosterScenario): RosterBackend {
  const { handlers, ...controls } = aSessionAgentRosterFake(scenario);
  return {
    backend: anInMemoryRpcBackend().implement(ConnectionService, handlers),
    ...controls,
  };
}

/**
 * The roster entry a daemon mints for a newly attached agent: what the offer said about it, under
 * the qualified id the attach carried. An id nobody offered still becomes a row — the daemon knows
 * its own defs even when this scenario declared no picker offers.
 */
function attachedEntryFor(
  agentId: string,
  offers: ReturnType<typeof anAvailableAgent>[],
): SessionAgentEntry {
  const offered = offers.find((o) => o.agentId === agentId);
  if (offered === undefined) return anAttachedAgent(agentId);
  return anAttachedAgent(agentId, {
    label: offered.label,
    model: offered.model,
    replaces: offered.replaces,
  });
}

/**
 * The shared tail behind every open roster stream. Each subscriber walks one list at its own
 * cursor, so a remount sees the same history rather than splitting it, and the generator never
 * returns — the daemon holds this stream for the session's life.
 */
function aRosterTail() {
  const pushed: Array<{ agents: SessionAgentEntry[]; rev: number }> = [];
  const wakers = new Set<() => void>();
  let current: { agents: SessionAgentEntry[]; rev: number } = { agents: [], rev: 0 };

  return {
    seed(agents: SessionAgentEntry[], rev: number) {
      current = { agents, rev };
    },
    currentAgents: () => current.agents,
    currentRev: () => current.rev,
    push(agents: SessionAgentEntry[], rev: number) {
      current = { agents, rev };
      pushed.push(current);
      const waiting = [...wakers];
      wakers.clear();
      for (const wake of waiting) wake();
    },
    async *frames() {
      let cursor = 0;
      for (;;) {
        while (cursor < pushed.length) {
          const frame = pushed[cursor];
          yield create(SessionAgentRosterSchema, {
            sessionId: "",
            rev: BigInt(frame.rev),
            agents: frame.agents,
          });
          cursor += 1;
        }
        await new Promise<void>((resolve) => wakers.add(resolve));
      }
    },
  };
}

/** A daemon that offers `agents` when the picker fans out to it. */
export function aDaemonOfferingAgents(
  agents: ReturnType<typeof anAvailableAgent>[],
): InMemoryRpcBackend {
  return anInMemoryRpcBackend().onUnary(ConnectionService.method.listSubagents, () => ({
    subagents: agents,
  }));
}

/** A daemon that cannot answer the picker's fan-out. */
export function aDaemonThatCannotBeReached(message: string): InMemoryRpcBackend {
  return anInMemoryRpcBackend().failWith(
    ConnectionService.method.listSubagents,
    Code.Unavailable,
    message,
  );
}
