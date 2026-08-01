import { describe, it, expect } from "bun:test";
import { formatAttachmentBytes } from "./attachmentBytes";

/**
 * Every size the attach rows show — a row's own byte count and the cap named in a refusal — goes
 * through this formatter, and the acceptance specs pin its exact output (`11 B`, `8 MiB`). Those
 * strings are the reason it exists at all: the decimal `formatBytes` used by the traffic strip renders
 * the same numbers as `11 B` and `8.4 MB`, so a daemon's power-of-two `max_attachment_bytes` would be
 * reported as a limit nobody configured.
 *
 * Changeset: `2026-08-01-session-attach-ui`
 */
describe("formatAttachmentBytes", () => {
  it("renders a small file in bytes, the size an attach row shows for an 11-byte spec", () => {
    // Given / When
    const rendered = formatAttachmentBytes(11);

    // Then
    expect(rendered).toEqual("11 B");
  });

  it("renders an empty file as zero bytes rather than as no size at all", () => {
    // Given / When
    const rendered = formatAttachmentBytes(0);

    // Then
    expect(rendered).toEqual("0 B");
  });

  it("keeps a size one byte below the first boundary in bytes", () => {
    // Given / When
    const rendered = formatAttachmentBytes(1023);

    // Then
    expect(rendered).toEqual("1023 B");
  });

  it("steps up to kibibytes at 1024 bytes, without a decimal point for an exact multiple", () => {
    // Given / When
    const rendered = formatAttachmentBytes(1024);

    // Then
    expect(rendered).toEqual("1 KiB");
  });

  it("keeps one decimal place just above an exact unit, so it does not read as exact", () => {
    // Given / When
    const rendered = formatAttachmentBytes(1025);

    // Then
    expect(rendered).toEqual("1.0 KiB");
  });

  it("renders the daemon's default-shaped cap as the 8 MiB a refusal names", () => {
    // Given / When
    const rendered = formatAttachmentBytes(8 * 1024 * 1024);

    // Then
    expect(rendered).toEqual("8 MiB");
  });

  it("rounds a size that is not a whole unit to one decimal place", () => {
    // Given — 1,500,000 bytes is 1.430511… MiB
    // When
    const rendered = formatAttachmentBytes(1_500_000);

    // Then
    expect(rendered).toEqual("1.4 MiB");
  });

  it("steps up to gibibytes at 1024 mebibytes", () => {
    // Given / When
    const rendered = formatAttachmentBytes(1024 * 1024 * 1024);

    // Then
    expect(rendered).toEqual("1 GiB");
  });

  it("counts tebibytes rather than inventing a unit beyond the largest it knows", () => {
    // Given — 1024 TiB, one step past the top of the unit table
    // When
    const rendered = formatAttachmentBytes(1024 ** 5);

    // Then
    expect(rendered).toEqual("1024 TiB");
  });
});
