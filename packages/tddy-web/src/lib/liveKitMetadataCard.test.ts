import { describe, it, expect } from "bun:test";
import { metadataCardText } from "./liveKitMetadataCard";

/**
 * The card has three answers for the one string a participant publishes: a pretty-printed document,
 * the raw string, or a note that there is nothing to show.
 */
describe("metadataCardText", () => {
  it("pretty-prints a metadata document that parses as JSON", () => {
    // Given / When
    const text = metadataCardText('{"owned_project_count":3}');

    // Then
    expect(text).toEqual('{\n  "owned_project_count": 3\n}');
  });

  it("shows metadata that is not JSON verbatim", () => {
    // Given / When
    const text = metadataCardText("not-json");

    // Then
    expect(text).toEqual("not-json");
  });

  it("states that nothing was published for an empty metadata string", () => {
    // Given / When
    const text = metadataCardText("");

    // Then
    expect(text).toEqual("No metadata published.");
  });

  it("states that nothing was published for whitespace-only metadata", () => {
    // Given / When
    const text = metadataCardText("   \n ");

    // Then
    expect(text).toEqual("No metadata published.");
  });
});
