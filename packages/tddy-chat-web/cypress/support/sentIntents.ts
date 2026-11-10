/**
 * Reads the intents a recording backend captured: asserts exactly one was sent, of the expected
 * kind, and returns its value typed for that kind — so a spec asserts on `answer.index` without
 * re-narrowing the `ClientMessage` intent union itself.
 */
import type { ClientMessage } from "tddy-client-web/gen/tddy/v1/remote_pb";

type Intent = ClientMessage["intent"];
type IntentCase = NonNullable<Intent["case"]>;
type IntentValue = { [K in IntentCase]: Extract<Intent, { case: K }>["value"] };

export function theOnlyIntent<C extends IntentCase>(
  intents: readonly ClientMessage[],
  kind: C,
): IntentValue[C] {
  expect(intents, "intents sent").to.have.length(1);
  const intent = intents[0]!.intent;
  expect(intent.case, "intent kind").to.equal(kind);
  return intent.value as IntentValue[C];
}
