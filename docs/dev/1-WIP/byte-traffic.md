# Changeset: byte-traffic — ConnectRPC byte-traffic interceptors + session traffic strip

**Date:** 2026-06-26  
**Branch:** `byte-traffic`  
**Packages:** `tddy-web`, `tddy-livekit-web`  
**Feature PRD:** [docs/ft/web/session-drawer.md § Session Traffic Strip](../../ft/web/session-drawer.md#session-traffic-strip)

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [ ] Implement `TrafficMeter` + `TrafficMeterRegistry` (`tddy-web/src/rpc/trafficMeter.ts`)
- [ ] Implement HTTP `Interceptor` wrapper (`tddy-web/src/rpc/httpTrafficInterceptor.ts`)
- [ ] Wire HTTP interceptor into `createDefaultHttpTransport` (`rpc/transportProvider.tsx`)
- [ ] Add optional `meter` to `LiveKitTransportOptions` and wire into `LiveKitTransport` (`tddy-livekit-web/src/transport.ts`)
- [ ] Wire LiveKit meter into `createDefaultLiveKitTransport` (`rpc/transportProvider.tsx`)
- [ ] Implement `readRoomRtt` + `useLiveKitPing` (`tddy-web/src/rpc/livekitPing.ts`)
- [ ] Implement `useSessionLiveKitRoom` (`tddy-web/src/components/sessions/useSessionLiveKitRoom.ts`)
- [ ] Implement format helpers (`tddy-web/src/components/sessions/formatTraffic.ts`)
- [ ] Implement `SessionTrafficStrip` component (`tddy-web/src/components/sessions/SessionTrafficStrip.tsx`)
- [ ] Wire strip into `SessionMainPane` (`tddy-web/src/components/sessions/SessionMainPane.tsx`)

## Acceptance tests

- [ ] `packages/tddy-web/cypress/component/SessionTrafficStrip.cy.tsx`
- [ ] `packages/tddy-web/cypress/component/SessionMainPaneTraffic.cy.tsx`

## Unit tests

- [ ] `packages/tddy-web/src/rpc/trafficMeter.test.ts`
- [ ] `packages/tddy-web/src/rpc/httpTrafficInterceptor.test.ts`
- [ ] `packages/tddy-livekit-web/src/transport.test.ts`
- [ ] `packages/tddy-web/src/rpc/livekitPing.test.ts`
- [ ] `packages/tddy-web/src/components/sessions/formatTraffic.test.ts`

## Validation Results

### validate-changes (2026-06-26)

**Critical (1):** Live meter wiring is incomplete — strip shows static zeros  
**Warning (1):** Empty `catch {}` in `httpTrafficInterceptor.ts` silently swallows serialization errors  
**Info (3):** `transportProvider.tsx` unmodified; `useSessionLiveKitRoom.ts` not created; all TODO items still unchecked

#### File-level notes

| File | Status | Notes |
|------|--------|-------|
| `trafficMeter.ts` | ✅ | Clean accumulator + sliding-window rate |
| `httpTrafficInterceptor.ts` | ⚠️ | Correct logic; empty catch acceptable but should be noted |
| `livekitPing.ts` | ✅ | Internal API access via `as any` is necessary; graceful null returns |
| `formatTraffic.ts` | ✅ | No issues |
| `SessionTrafficStrip.tsx` | ✅ | Presentational; correct testids and layout |
| `SessionMainPane.tsx` | 🔴 | Strip mounted with hardcoded zeros; meter not wired |
| `transport.ts` | ✅ | Minimal additive change; backward-compatible |
| `transportProvider.tsx` | 🔴 | Not modified — HTTP interceptor and LiveKit meter not wired in |
| `useSessionLiveKitRoom.ts` | 🔴 | Not created — needed to supply Room for ping + LiveKit meter |

---

## Delta summary

### `tddy-web`

**New files:**
- `src/rpc/trafficMeter.ts` — `TrafficMeter` (accumulator + 2 s sliding-window B/s rate,
  subscribe/notify), `TrafficMeterRegistry` (keyed by scope), React context +
  `useTrafficMeter(scope)` hook.
- `src/rpc/httpTrafficInterceptor.ts` — `createTrafficInterceptor(meter): Interceptor`:
  uses `toBinary(method.input/output, message)` to size unary req/res; wraps streaming
  async iterables for per-chunk counting.
- `src/rpc/livekitPing.ts` — `readRoomRtt(room): Promise<number | null>` (WebRTC
  `getStats()` `currentRoundTripTime` from the succeeded candidate-pair, converted to ms);
  `useLiveKitPing(room, intervalMs?)` hook.
- `src/components/sessions/useSessionLiveKitRoom.ts` — acquires a LiveKit `Room` for the
  currently attached session, mirrors `useCommonRoom.ts`.
- `src/components/sessions/formatTraffic.ts` — `formatBytes`, `formatRate`, `formatPing`.
- `src/components/sessions/SessionTrafficStrip.tsx` — presentational strip:
  `data-testid="session-traffic-strip"`, Tailwind `flex-shrink-0` row; shows ↓/↑ rates,
  totals, and ping.

**Modified files:**
- `src/rpc/transportProvider.tsx` — `createDefaultHttpTransport` gains
  `interceptors: [createTrafficInterceptor(httpMeter)]`; `createDefaultLiveKitTransport`
  passes the per-room meter to the `LiveKitTransport`.
- `src/components/sessions/SessionMainPane.tsx` — renders `SessionTrafficStrip` in the
  `connected-livekit` state above the inspector toggle row.

### `tddy-livekit-web`

**Modified files:**
- `src/transport.ts` — `LiveKitTransportOptions` gains optional
  `meter?: { record(dir: "in" | "out", bytes: number): void }`; `publishRequest` calls
  `meter.record("out", payload.length)`; `DataReceived` listener calls
  `meter.record("in", payload.length)`.
