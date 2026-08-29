/**
 * Pure merge rules for the Models & Agents screen's cross-daemon fan-out
 * (docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md § AC2, AC12).
 *
 * Every RPC in `models.ModelRegistryService` is scoped to the daemon serving it — there is no
 * cross-daemon forwarding — so the web reads each common-room daemon's registry and merges the
 * answers here, exactly as the sessions drawer does for its cross-host rows
 * (`utils/crossHostSessions.ts`). Kept free of React and of JSX so `bun test` can run it directly.
 *
 * Two rules earn their own module:
 *   • a row's **owning daemon** is the id the serving daemon stamped on it, falling back to the
 *     daemon the row was read from — the row's owner is what per-row actions are routed to, so
 *     getting it wrong sends a Load to the wrong host;
 *   • a daemon whose registry could not be read contributes a **failure**, not an empty registry,
 *     so one unreachable peer is reported instead of silently shrinking the table. The failure is
 *     reported *alongside* whatever rows did arrive, because the registry is read as four separate
 *     lists: a daemon whose assistants cannot be read still knows its models, and blanking them
 *     would claim a host has nothing when only one of its four answers was lost.
 */

import type { ModelLoadState, ProviderKind } from "../gen/models_pb";

// ---------------------------------------------------------------------------
// View rows
// ---------------------------------------------------------------------------

/** A provider, resolved to its owning daemon. */
export interface ProviderRow {
  readonly daemonInstanceId: string;
  readonly providerId: string;
  readonly kind: ProviderKind;
  readonly label: string;
  readonly baseUrl: string;
  readonly hasCredential: boolean;
  /** Non-empty when the daemon's last enumeration of this provider failed. */
  readonly enumerationError: string;
}

/** A model, resolved to its owning daemon. */
export interface ModelRow {
  readonly daemonInstanceId: string;
  readonly providerId: string;
  readonly modelId: string;
  readonly label: string;
  readonly labels: readonly string[];
  readonly loadState: ModelLoadState;
  readonly sizeBytes: bigint;
}

/** An assistant, resolved to its owning daemon. */
export interface AssistantRow {
  readonly daemonInstanceId: string;
  readonly assistantId: string;
  readonly name: string;
  readonly label: string;
  readonly providerId: string;
  readonly modelId: string;
  readonly systemPrompt: string;
  /** What this assistant may call while it works. */
  readonly tools: readonly string[];
  /** The main-agent tools it stands in for — what attaching it takes away from the session. */
  readonly replaces: readonly string[];
}

/** A daemon whose registry could not be read, and why. */
export interface DaemonFailure {
  readonly instanceId: string;
  readonly error: string;
}

/** One daemon's answer to the registry fan-out: the rows that arrived, and what did not arrive. */
export interface DaemonRegistrySnapshot {
  readonly instanceId: string;
  readonly providers: readonly ProviderRow[];
  readonly models: readonly ModelRow[];
  readonly assistants: readonly AssistantRow[];
  /** Empty when every list arrived; which lists failed, and why, otherwise. */
  readonly error: string;
}

/** The fleet-wide registry the screen renders. */
export interface MergedRegistry {
  readonly providers: ProviderRow[];
  readonly models: ModelRow[];
  readonly assistants: AssistantRow[];
  readonly failures: DaemonFailure[];
}

// ---------------------------------------------------------------------------
// Rules
// ---------------------------------------------------------------------------

/**
 * The daemon a row belongs to: the id the serving daemon stamped on it, else the daemon it was read
 * from. Mirrors `owningHostForSession` — a row with no stamp is owned by whoever answered for it.
 */
export function owningDaemonOf(stampedInstanceId: string, sourceInstanceId: string): string {
  return stampedInstanceId.trim() || sourceInstanceId;
}

/**
 * Merge every daemon's snapshot into one registry, preserving daemon order and each daemon's own
 * row order. A daemon that reported an error contributes exactly one failure, plus whatever rows it
 * did answer with — a daemon that answered nothing carries no rows, so a wholly unreachable peer
 * still merges as a failure and nothing else.
 */
export function mergeRegistryEntries(
  snapshots: readonly DaemonRegistrySnapshot[],
): MergedRegistry {
  const providers: ProviderRow[] = [];
  const models: ModelRow[] = [];
  const assistants: AssistantRow[] = [];
  const failures: DaemonFailure[] = [];

  for (const snapshot of snapshots) {
    if (snapshot.error !== "") {
      failures.push({ instanceId: snapshot.instanceId, error: snapshot.error });
    }
    providers.push(...snapshot.providers);
    models.push(...snapshot.models);
    assistants.push(...snapshot.assistants);
  }

  return { providers, models, assistants, failures };
}

/** One list of a daemon's registry that could not be read, and the daemon's own words for why. */
export interface RegistryListFailure {
  /** The list that failed, as the operator knows it: `providers`, `models`, `assistants`, `tools`. */
  readonly list: string;
  readonly message: string;
}

/**
 * One line describing every list of a daemon's registry that failed, grouped by message so an
 * unreachable daemon reads `providers, models, assistants, tools: …` once rather than repeating the
 * same sentence four times. Empty when nothing failed.
 */
export function describeReadFailures(failures: readonly RegistryListFailure[]): string {
  const listsByMessage = new Map<string, string[]>();
  for (const { list, message } of failures) {
    const lists = listsByMessage.get(message);
    if (lists) lists.push(list);
    else listsByMessage.set(message, [list]);
  }
  return [...listsByMessage]
    .map(([message, lists]) => `${lists.join(", ")}: ${message}`)
    .join("; ");
}

/** The stable identity of a model row across the merged table — also its `data-testid` key. */
export function modelRowKey(model: {
  daemonInstanceId: string;
  providerId: string;
  modelId: string;
}): string {
  return `${model.daemonInstanceId}/${model.providerId}/${model.modelId}`;
}

/**
 * The stable identity of a provider across the merged table. Provider ids are minted per daemon, so
 * `prov-ollama` exists on every host — keying anything by the bare id would render one daemon's
 * enumeration error against another daemon's identically named provider.
 */
export function providerRowKey(provider: {
  daemonInstanceId: string;
  providerId: string;
}): string {
  return `${provider.daemonInstanceId}/${provider.providerId}`;
}

/**
 * The stable identity of an assistant across the merged panel. Assistant names are unique *per
 * daemon* — two hosts may each define a `reviewer` — so the owning daemon is part of the key, or
 * one host's row would be rendered against the other's.
 */
export function assistantRowKey(assistant: {
  daemonInstanceId: string;
  name: string;
}): string {
  return `${assistant.daemonInstanceId}/${assistant.name}`;
}

/**
 * How far along the fleet-wide read is. The screen has to tell "still reading" and "nowhere to read
 * from" apart from "the fleet has no models" — an empty table means all three otherwise.
 */
export type RegistryReadStatus = "not-connected" | "no-daemons" | "loading" | "ready";

/**
 * What a registry panel says when it holds no rows, and why it holds none. Only the last state is
 * the panel's own claim ("this fleet has none"), so the caller supplies the wording for that one and
 * for what it is reading; the two connection-level answers are the same for every panel.
 */
export function registryEmptyStateText(
  status: RegistryReadStatus,
  texts: { readonly loading: string; readonly ready: string },
): string {
  switch (status) {
    case "not-connected":
      return "Not connected to the common room";
    case "no-daemons":
      return "No daemons in the common room";
    case "loading":
      return texts.loading;
    default:
      return texts.ready;
  }
}

/** {@link RegistryReadStatus} from the connection, the daemons in the room, and the reads so far. */
export function registryReadStatus(params: {
  /** Whether the shared common-room connection exists — without it no daemon can be addressed. */
  connected: boolean;
  daemonCount: number;
  /** How many of those daemons have answered (with rows or with an error). */
  answeredCount: number;
}): RegistryReadStatus {
  if (!params.connected) return "not-connected";
  if (params.daemonCount === 0) return "no-daemons";
  if (params.answeredCount < params.daemonCount) return "loading";
  return "ready";
}
