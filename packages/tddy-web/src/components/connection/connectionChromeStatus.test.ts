import { describe, expect, test } from "bun:test";
import { dataConnectionStatusValue } from "./connectionChromeStatus";

describe("dataConnectionStatusValue", () => {
  test("maps connecting", () => {
    expect(dataConnectionStatusValue("connecting")).toBe("connecting");
  });
  test("maps connected", () => {
    expect(dataConnectionStatusValue("connected")).toBe("connected");
  });
  test("maps error", () => {
    expect(dataConnectionStatusValue("error")).toBe("error");
  });
});
