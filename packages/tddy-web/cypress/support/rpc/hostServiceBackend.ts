/**
 * In-memory `host.HostService` backend — the daemon roster and the host telemetry feed.
 *
 * Split out of `daemonSessionHostBackend.ts` when the eight host RPCs left
 * `session.SessionService` for `host.HostService`: a fake is registered per service, so a
 * screen's backend composes the two rather than one `.implement` covering both. `aHostServiceFake`
 * is the composable half (`handlers` spread into a caller's own `.implement(HostService, …)`),
 * `aHostServiceBackend` the standalone one, mirroring `sessionAgentRosterBackend.ts`.
 *
 * The prompt half of the service — `StreamHostPrompts` / `AnswerHostPrompt` — is `hostPromptFeed.ts`,
 * which composes into the same `HostService` implementation.
 */

import { create } from "@bufbuild/protobuf";
import type { ServiceImpl } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  EligibleDaemonEntrySchema,
  HostCpuStatsSchema,
  HostDiskStatsSchema,
  HostLoadStatsSchema,
  HostMemoryStatsSchema,
  HostService,
  HostStatsEventSchema,
  type EligibleDaemonEntry,
} from "../../../src/gen/host_pb";

/** One entry of the `ListEligibleDaemons` roster. */
export interface DaemonEntry {
  instanceId: string;
  label: string;
  isLocal: boolean;
}

/** Canonical daemon pair for multi-host tests (same values as the retired `connectionRpcs.ts`). */
export const DAEMON_LOCAL: DaemonEntry = {
  instanceId: "workstation-1",
  label: "workstation-1 (this daemon)",
  isLocal: true,
};

export const DAEMON_PEER: DaemonEntry = {
  instanceId: "server-2",
  label: "server-2",
  isLocal: false,
};

export function anEligibleDaemonEntry(overrides: Partial<EligibleDaemonEntry>): EligibleDaemonEntry {
  return create(EligibleDaemonEntrySchema, {
    instanceId: "local",
    label: "local (this daemon)",
    isLocal: true,
    ...overrides,
  });
}

export interface HostServiceScenario {
  /** ListEligibleDaemons response. Defaults to a single local daemon. */
  daemons?: DaemonEntry[];
  /** First `StreamHostStats` event — per-logical-core utilization percentages (0..100). Default empty. */
  hostCpuPerCore?: number[];
  /** Every `StreamHostStats` event — free/total bytes for the daemon's default project directory. */
  hostDisk?: { availableBytes: bigint; totalBytes: bigint; projectDir: string };
  /** When set, `StreamHostStats` emits a second event carrying these per-core percentages after the
   *  first — lets a test assert the footer applies fresh readings streamed by the server. */
  hostCpuPerCoreUpdate?: number[];
  /** When true, `StreamHostStats` opens and then emits nothing — a host that is subscribed but has
   *  not reported yet, which is what a caller must render as pending rather than as zeroes. */
  hostStatsSilent?: boolean;
  /** Total/available memory the fake host reports. Omitted means it reports none. */
  hostMemoryBytes?: { availableBytes: bigint; totalBytes: bigint };
  /** 1/5/15-minute load averages. **Omitted models a platform that has no load average** — the
   *  case the UI must render as "no reading" rather than as 0.00. */
  hostLoadAverage?: { oneMinute: number; fiveMinutes: number; fifteenMinutes: number };
}

export interface HostServiceControls {
  /** Number of times `StreamHostStats` was subscribed — lets a test assert the footer opens the
   *  single host-stats stream exactly once. */
  readonly hostStatsStreamCount: () => number;
}

/** The host fake as handlers, so a screen's own backend can serve it too. */
export interface HostServiceFake extends HostServiceControls {
  handlers: Partial<ServiceImpl<typeof HostService>>;
}

/**
 * Build the `host.HostService` handlers for `scenario`, plus the recorders a spec asserts on.
 *
 * `StreamHostStats` emits one event carrying both CPU and disk immediately (the server's
 * on-subscribe snapshot), optionally a second event with updated CPU, then stays open — a completed
 * stream would look like the daemon dropping the feed.
 */
export function aHostServiceFake(scenario: HostServiceScenario = {}): HostServiceFake {
  let hostStatsStreamOpens = 0;
  const daemons = scenario.daemons ?? [
    { instanceId: "local", label: "local (this daemon)", isLocal: true },
  ];

  const handlers: Partial<ServiceImpl<typeof HostService>> = {
    listEligibleDaemons: async () => ({
      daemons: daemons.map((d) => anEligibleDaemonEntry(d)),
    }),
    streamHostStats: async function* () {
      hostStatsStreamOpens += 1;
      // A silent feed stays open without ever reporting — the state a caller must render as
      // pending rather than as zeroes.
      if (scenario.hostStatsSilent) {
        await new Promise<never>(() => undefined);
      }
      const disk = create(HostDiskStatsSchema, {
        availableBytes: scenario.hostDisk?.availableBytes ?? 0n,
        totalBytes: scenario.hostDisk?.totalBytes ?? 0n,
        projectDir: scenario.hostDisk?.projectDir ?? "",
      });
      const memory = scenario.hostMemoryBytes
        ? create(HostMemoryStatsSchema, {
            availableBytes: scenario.hostMemoryBytes.availableBytes,
            totalBytes: scenario.hostMemoryBytes.totalBytes,
          })
        : undefined;
      // Left `undefined` when the scenario names no load average, so the fake reproduces a host
      // whose platform has none — the case the UI must not render as 0.00.
      const load = scenario.hostLoadAverage
        ? create(HostLoadStatsSchema, {
            oneMinute: scenario.hostLoadAverage.oneMinute,
            fiveMinutes: scenario.hostLoadAverage.fiveMinutes,
            fifteenMinutes: scenario.hostLoadAverage.fifteenMinutes,
          })
        : undefined;
      const cores = (scenario.hostCpuPerCore ?? []).length;
      yield create(HostStatsEventSchema, {
        cpu: create(HostCpuStatsSchema, {
          perCorePercent: scenario.hostCpuPerCore ?? [],
          logicalCores: cores,
        }),
        disk,
        memory,
        load,
      });
      if (scenario.hostCpuPerCoreUpdate) {
        yield create(HostStatsEventSchema, {
          cpu: create(HostCpuStatsSchema, {
            perCorePercent: scenario.hostCpuPerCoreUpdate,
            logicalCores: scenario.hostCpuPerCoreUpdate.length,
          }),
          disk,
          memory,
          load,
        });
      }
      await new Promise<never>(() => undefined);
    },
  };

  return { handlers, hostStatsStreamCount: () => hostStatsStreamOpens };
}

export interface HostServiceBackend extends HostServiceControls {
  backend: InMemoryRpcBackend;
}

/** The same fake as a backend of its own, for a spec that needs nothing but `host.HostService`. */
export function aHostServiceBackend(scenario: HostServiceScenario = {}): HostServiceBackend {
  const { handlers, ...controls } = aHostServiceFake(scenario);
  return {
    backend: anInMemoryRpcBackend().implement(HostService, handlers),
    ...controls,
  };
}
