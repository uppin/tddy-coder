import { describe, expect, test } from "bun:test";
import { rpcBaseToHttpOrigin } from "./wait-for-daemon-rpc";

describe("rpcBaseToHttpOrigin", () => {
  test("strips path", () => {
    expect(rpcBaseToHttpOrigin("http://127.0.0.1:8899/rpc")).toBe(
      "http://127.0.0.1:8899"
    );
  });

  test("returns null on garbage", () => {
    expect(rpcBaseToHttpOrigin("not a url")).toBeNull();
  });
});
