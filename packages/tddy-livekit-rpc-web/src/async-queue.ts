/**
 * `AsyncQueue` now lives in `tddy-rpc-web` — it is the streaming channel every envelope flavour
 * uses, not a LiveKit detail. Re-exported here so this package's own modules and its consumers keep
 * one import path for it.
 */
export { AsyncQueue } from "tddy-rpc-web";
