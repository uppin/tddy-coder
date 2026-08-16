/**
 * Helpers for asserting on the requests an `InMemoryRpcBackend` recorded.
 *
 * `backend.callsTo(method)` hands back the request exactly as ConnectRPC built it — a
 * `@bufbuild/protobuf` message, which carries an own-enumerable `$typeName` marker alongside the
 * declared fields. Chai's `deep.equal` compares key sets, so comparing a recorded request directly
 * against an object literal can never match, however right the literal is.
 *
 * `requestFields` drops that marker so a spec can keep asserting the *whole* request by exact
 * equality — which is what we want, since a partial assertion would let an unexpected extra field
 * through unnoticed.
 */

/** A recorded request's declared fields, without protobuf's `$typeName` marker. */
export function requestFields<T extends object>(message: T): Omit<T, "$typeName"> {
  const { $typeName: _typeName, ...fields } = message as T & { $typeName?: string };
  return fields as Omit<T, "$typeName">;
}

/** Every recorded request for a method, as plain field objects, in call order. */
export function recordedFields<T extends object>(calls: readonly T[]): Array<Omit<T, "$typeName">> {
  return calls.map(requestFields);
}
