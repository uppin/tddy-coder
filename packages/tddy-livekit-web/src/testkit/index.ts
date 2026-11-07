/**
 * Fakes for the LiveKit layer: rooms a spec can seed, and a room that cannot be joined. Specs outside
 * this package use these in place of `new Room()`, so they never import `livekit-client`.
 */
export * from "./fakeCommonRoom";
export * from "./commonRoomConnection";
export * from "./aRoom";
