/**
 * How the web renders a protobuf enum value it has no name for.
 *
 * A daemon may be newer than the browser tab talking to it — it is deployed independently — so an
 * enum can carry a value this build has never heard of. Substituting a friendly word like "Unknown"
 * makes version skew read as ordinary data, and an operator then debugs a provider that is fine.
 * The raw value is rendered instead, together with what it means that the value is unrecognised.
 */

/**
 * `Unrecognised <what> <value> — the daemon sent a value this web build has no name for`.
 *
 * States what happened rather than why: an unset field and a daemon newer than the tab both arrive
 * this way, and the web cannot tell them apart.
 */
export function unrecognisedEnumText(what: string, value: number): string {
  return `Unrecognised ${what} ${value} — the daemon sent a value this web build has no name for`;
}
