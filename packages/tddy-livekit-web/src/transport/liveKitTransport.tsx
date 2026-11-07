/**
 * The LiveKit transport factory every room-addressed RPC is built with.
 *
 * The app owns what wraps a transport (the auth gate, the traffic meter) and mounts
 * {@link LiveKitTransportProvider} with the result; this package owns what the transport *is*
 * (`createLiveKitTransport` over a `Room`). With no provider in scope the default factory is used
 * unwrapped, exactly as a component rendered outside the app's transport provider always got it.
 */

import { createContext, useContext, type ReactNode } from "react";
import type { Transport } from "@connectrpc/connect";
import { createLiveKitTransport } from "tddy-livekit-rpc-web";
import type { RoomHandle } from "tddy-client-web";
import { asRoom } from "../roomHandle";

/** Options forwarded to the LiveKit transport factory. */
export interface RoomTransportOptions {
  /** Enable debug logging in the transport. */
  debug?: boolean;
}

/** The peer half of the transport seam: a transport to `targetIdentity` over `room`. */
export type RoomTransportFactory = (room: RoomHandle, targetIdentity: string, options?: RoomTransportOptions) => Transport;

/** True when the active debug mask enables `tddy:rpc*` — turns on the transport's per-RPC console logging. */
function rpcDebugActive(): boolean {
  try {
    return (window.localStorage.getItem("debug") ?? "").includes("tddy:rpc");
  } catch {
    return false;
  }
}

/** Where a transport records its payload bytes. */
export interface TrafficMeter {
  record(dir: "in" | "out", bytes: number): void;
}

/** What the app lends a transport for its traffic meter: the meter of one room, by name. */
export type MeterFor = (roomName: string) => TrafficMeter | undefined;

/**
 * Factory for the production LiveKit transport. Exported so a test can drive the real transport the
 * app runs with — in particular its fault reporting, which has no other observable surface.
 */
export function createDefaultLiveKitTransport(
  handle: RoomHandle,
  targetIdentity: string,
  options?: RoomTransportOptions,
  meterFor?: MeterFor,
): Transport {
  const room = asRoom(handle);
  if (room === null) throw new Error("createDefaultLiveKitTransport needs a joined room");
  return createLiveKitTransport({
    room,
    targetIdentity,
    debug: options?.debug ?? rpcDebugActive(),
    meter: meterFor?.(room.name || "livekit"),
    // An inbound frame the transport cannot turn into a response is a dropped RPC: the call never
    // settles, and the app sees only a stalled or throwing refresh. Name the connection it arrived
    // on so the console says which room and which daemon, not just that decoding failed.
    onTransportError: (error, context) =>
      console.error(`[tddy][rpc] room=${room.name} target=${targetIdentity}: ${context}`, error),
  });
}

interface LiveKitTransportContextValue {
  readonly factory: RoomTransportFactory;
  /**
   * True when `factory` is a test double (an explicit override of the app's transport provider).
   * The production default needs a genuinely connected `Room` and crashes without one; a double may
   * ignore its room entirely, so a caller that may run before a room exists can tell "no room yet,
   * and a double that does not care" from "no room yet, for real" (see `useRoomClient`).
   */
  readonly overridden: boolean;
}

const DEFAULT_VALUE: LiveKitTransportContextValue = { factory: (room, identity, options) => createDefaultLiveKitTransport(room, identity, options), overridden: false };

const LiveKitTransportContext = createContext<LiveKitTransportContextValue>(DEFAULT_VALUE);

export function LiveKitTransportProvider(props: { factory: RoomTransportFactory; overridden: boolean; children?: ReactNode }) {
  const { factory, overridden, children } = props;
  return <LiveKitTransportContext.Provider value={{ factory, overridden }}>{children}</LiveKitTransportContext.Provider>;
}

/** The LiveKit transport factory in scope (the default, unwrapped, with no provider). */
export function useLiveKitTransportFactory(): RoomTransportFactory {
  return useContext(LiveKitTransportContext).factory;
}

/** Whether the factory in scope is a test double — see {@link LiveKitTransportContextValue.overridden}. */
export function useLiveKitTransportFactoryIsOverridden(): boolean {
  return useContext(LiveKitTransportContext).overridden;
}
