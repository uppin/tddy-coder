import { describe, expect, it } from "bun:test";
import { dataConnectionStatusValue } from "./connectionChromeStatus";

describe("dataConnectionStatusValue", () => {
  it("returns 'connecting' as the data-attribute value for a connecting state", () => {
    // When
    const result = dataConnectionStatusValue("connecting");
    // Then
    expect(result).toBe("connecting");
  });

  it("returns 'connected' as the data-attribute value for a connected state", () => {
    // When
    const result = dataConnectionStatusValue("connected");
    // Then
    expect(result).toBe("connected");
  });

  it("returns 'error' as the data-attribute value for an error state", () => {
    // When
    const result = dataConnectionStatusValue("error");
    // Then
    expect(result).toBe("error");
  });
});
