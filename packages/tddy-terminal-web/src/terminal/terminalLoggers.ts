import { tddyDebug } from "tddy-client-web";

// Namespaced `[tddy]` diagnostics enabled by the DEBUG mask (daemon `debug:` / localStorage.debug).
// Scoped to the data flow behind terminal garbling / misalignment.
const dLife = tddyDebug("tddy:term:life");
const dData = tddyDebug("tddy:term:data");
export const dWrite = tddyDebug("tddy:term:write");
const dResize = tddyDebug("tddy:term:resize");
const dMouse = tddyDebug("tddy:term:mouse");
const dScroll = tddyDebug("tddy:term:scroll");

type LogFn = (fmt: string, ...args: unknown[]) => void;

export interface TerminalLoggers {
  logLife: LogFn;
  logData: LogFn;
  logWrite: LogFn;
  logResize: LogFn;
  logMouse: LogFn;
  logScroll: LogFn;
}

/**
 * Each logger fires when its namespace is in the active DEBUG mask, OR unconditionally when the
 * legacy `debugLogging` prop is set (standalone "Debug logging" checkbox).
 */
export function createTerminalLoggers(debugLogging: boolean): TerminalLoggers {
  const mkLog =
    (d: ReturnType<typeof tddyDebug>): LogFn =>
    (fmt: string, ...args: unknown[]) => {
      if (debugLogging) console.debug(`[${d.namespace}]`, fmt, ...args);
      else if (d.enabled) d(fmt, ...args);
    };
  return {
    logLife: mkLog(dLife),
    logData: mkLog(dData),
    logWrite: mkLog(dWrite),
    logResize: mkLog(dResize),
    logMouse: mkLog(dMouse),
    logScroll: mkLog(dScroll),
  };
}
