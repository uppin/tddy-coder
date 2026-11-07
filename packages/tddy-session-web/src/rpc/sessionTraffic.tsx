/**
 * The traffic seam: what the status bar needs from the app to read the wire.
 *
 * The control-plane byte meter belongs to the app's HTTP transport, and finding the LiveKit room a
 * session connection runs over needs LiveKit, which a session package never imports. The app supplies
 * both through {@link SessionTrafficProvider}; without one the status bar reads no control-plane
 * traffic and no round-trip time — the same "no provider, show zeros" reading it always had.
 */

import React, { createContext, useContext, type ReactNode } from "react";
import type { SessionConnection } from "tddy-client-web/connections/session";
import type { TrafficMeterRegistry } from "./trafficMeter";

export interface SessionTraffic {
  /** The meters the app's transports record into, or `null` when the app keeps none. */
  readonly meterRegistry: TrafficMeterRegistry | null;
  /** The LiveKit room `connection` runs over, for round-trip timing; `null` when it has none. */
  readonly roomOf: (connection: SessionConnection | null) => unknown | null;
}

const NO_TRAFFIC: SessionTraffic = { meterRegistry: null, roomOf: () => null };

const SessionTrafficContext = createContext<SessionTraffic>(NO_TRAFFIC);

/** Supplies the app's meters and room lookup to the status bar below it. */
export function SessionTrafficProvider({
  traffic,
  children,
}: {
  traffic: SessionTraffic;
  children?: ReactNode;
}): React.ReactElement {
  return <SessionTrafficContext.Provider value={traffic}>{children}</SessionTrafficContext.Provider>;
}

/** The traffic the app supplied; an inert reading when no {@link SessionTrafficProvider} is above. */
export function useSessionTraffic(): SessionTraffic {
  return useContext(SessionTrafficContext);
}
