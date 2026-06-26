/**
 * Browser DEBUG mask wiring for `[tddy]` diagnostics, built on the `debug` package.
 *
 * The daemon exposes a `debug:` mask via `GET /api/config` (see `dev.daemon.yaml`). That value is the
 * source of truth: when it changes, any per-browser override in `localStorage.debug` is invalidated and
 * the new config mask is adopted. While the config mask is unchanged, the persisted active mask wins, so
 * a developer can tweak namespaces live (DevTools: `localStorage.debug = 'tddy:term:write'`) and it sticks
 * across sessions. A `?debug=` URL param applies immediately for the current session.
 *
 * Namespaces are dot-scoped under `tddy:term:*` (e.g. `tddy:term:write`, `tddy:term:resize`,
 * `tddy:term:grpc`) so masks can target the data flow behind terminal garbling / misalignment.
 */
import createDebug from "debug";

/** localStorage key the `debug` package reads for the active mask. */
const ACTIVE_KEY = "debug";
/** localStorage key recording the last config mask we applied (for change detection). */
const CONFIG_BASE_KEY = "tddy:debug:configBase";

export interface DebugMaskInputs {
  /** Mask from `GET /api/config` (daemon `debug:`); undefined/empty means unset. */
  configMask?: string | null;
  /** Last config mask we applied, read from localStorage. */
  storedBase: string | null;
  /** Current active mask (`localStorage.debug`). */
  storedActive: string | null;
}

export interface DebugMaskResolution {
  /** New active mask, or `null` to clear it. Only written when `changed` is true. */
  activeMask: string | null;
  /** True when the config mask changed and the active mask must be (re)written/cleared. */
  changed: boolean;
  /** New value to record as the config base. */
  base: string;
}

/** Trim a mask; treat null/undefined/blank as empty string. */
function norm(mask?: string | null): string {
  return (mask ?? "").trim();
}

/**
 * Pure resolution of the active debug mask. The config mask is authoritative: when it differs from the
 * recorded base the active mask is reset to it (invalidating any prior per-browser override); otherwise
 * the persisted active mask is kept. See module docs for the precedence rationale.
 */
export function resolveDebugMask(inputs: DebugMaskInputs): DebugMaskResolution {
  const config = norm(inputs.configMask);
  const base = norm(inputs.storedBase);
  if (config !== base) {
    return { activeMask: config === "" ? null : config, changed: true, base: config };
  }
  const active = norm(inputs.storedActive);
  return { activeMask: active === "" ? null : active, changed: false, base: config };
}

/** Enable/disable the live `debug` runtime to match a resolved mask. */
function applyToRuntime(mask: string | null): void {
  if (mask && mask.length > 0) createDebug.enable(mask);
  else createDebug.disable();
}

function readLocalStorage(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function writeLocalStorage(key: string, value: string | null): void {
  try {
    if (value === null) localStorage.removeItem(key);
    else localStorage.setItem(key, value);
  } catch {
    /* localStorage unavailable (private mode / SSR) — runtime enable still applies */
  }
}

/**
 * Apply the daemon-provided mask, honouring the config-change invalidation rule, and sync the live
 * `debug` runtime. Safe to call before or after terminals mount — `debug` re-evaluates namespaces lazily.
 */
export function applyDebugMaskFromConfig(configMask?: string | null): void {
  const res = resolveDebugMask({
    configMask,
    storedBase: readLocalStorage(CONFIG_BASE_KEY),
    storedActive: readLocalStorage(ACTIVE_KEY),
  });
  writeLocalStorage(CONFIG_BASE_KEY, res.base);
  if (res.changed) writeLocalStorage(ACTIVE_KEY, res.activeMask);
  // Re-sync runtime with whatever the active mask is now (persisted override or freshly-adopted config).
  applyToRuntime(res.changed ? res.activeMask : readLocalStorage(ACTIVE_KEY));
}

/**
 * Apply a `?debug=` URL param for the current session: writes it as the active mask and enables it
 * immediately. Call once at boot, before the config fetch resolves. No-op when the param is absent.
 */
export function applyDebugMaskFromUrl(search: string = window.location.search): void {
  const param = new URLSearchParams(search).get("debug");
  if (param === null) return;
  const mask = param.trim();
  writeLocalStorage(ACTIVE_KEY, mask === "" ? null : mask);
  applyToRuntime(mask === "" ? null : mask);
}

/** Create a namespaced `[tddy]` debug logger (enabled per the active mask). */
export function tddyDebug(namespace: string): createDebug.Debugger {
  return createDebug(namespace);
}

/** Force-enable a mask (used by the legacy `debugLogging` terminal prop / standalone checkbox). */
export function enableDebugMask(mask: string): void {
  applyToRuntime(mask);
}
