import { describe, expect, test } from "bun:test";
import { create } from "@bufbuild/protobuf";
import {
  ServerMessageSchema,
  SessionRuntimeStatusSchema,
} from "../gen/tddy/v1/remote_pb";

describe("SessionRuntimeStatus contract", () => {
  test("round-trips status_line and structured fields", () => {
    const inner = create(SessionRuntimeStatusSchema, {
      statusLine: "Goal: plan │ State: Running │ 1m 0s │ agent │ model",
      goal: "plan",
      workflowState: "Running",
      elapsedMs: 60_000n,
      agent: "agent",
      model: "model",
    });
    const msg = create(ServerMessageSchema, {
      event: {
        case: "sessionRuntimeStatus",
        value: inner,
      },
    });
    expect(msg.event?.case).toBe("sessionRuntimeStatus");
    if (msg.event?.case !== "sessionRuntimeStatus") return;
    expect(msg.event.value.statusLine).toContain("plan");
    expect(msg.event.value.goal).toBe("plan");
    expect(msg.event.value.workflowState).toBe("Running");
    expect(msg.event.value.elapsedMs).toBe(60_000n);
    expect(msg.event.value.agent).toBe("agent");
    expect(msg.event.value.model).toBe("model");
  });
});
