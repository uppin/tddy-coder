import { describe, expect, test } from "bun:test";
import { toScheduleViewModel } from "./SchedulesPanel";

describe("toScheduleViewModel", () => {
  test("carries the status, what it is held up on and each condition", () => {
    const model = toScheduleViewModel({
      itemId: "n2",
      kind: "propagate",
      status: "waiting",
      heldUpOn: "inbox",
      conditions: [{ name: "inbox", state: "waiting", detail: "nothing to take yet" }],
    });
    expect(model).toEqual({
      itemId: "n2",
      kind: "propagate",
      status: "waiting",
      heldUpOn: "inbox",
      conditions: [{ name: "inbox", state: "waiting", detail: "nothing to take yet" }],
    });
  });

  test("an empty held-up-on is none, and a state it cannot read is shown blocked", () => {
    const model = toScheduleViewModel({
      itemId: "n1",
      kind: "merge",
      status: "met",
      heldUpOn: "",
      conditions: [{ name: "order", state: "???", detail: "" }],
    });
    expect([model.heldUpOn, model.conditions[0].state]).toEqual([undefined, "blocked"]);
  });
});
