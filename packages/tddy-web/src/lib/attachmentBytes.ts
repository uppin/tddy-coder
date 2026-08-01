/**
 * Byte formatting for the Start-session attach rows and the host's advertised attachment cap.
 *
 * Binary units (KiB/MiB/GiB) rather than the decimal `formatBytes` used by the traffic strip: the
 * daemon's `max_attachment_bytes` is a power-of-two setting, so a decimal rendering would name a
 * limit ("8.4 MB") the operator never configured. One formatter for both the row size and the
 * refusal message keeps the two readings comparable.
 *
 * Changeset: `2026-08-01-session-attach-ui`
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-01-session-attach-ui.md
 */

const BINARY_UNITS = ["B", "KiB", "MiB", "GiB", "TiB"] as const;

/**
 * Formats a byte count in binary units, dropping the decimal for an exact multiple: `11` → `"11 B"`,
 * `8 * 1024 * 1024` → `"8 MiB"`, `1_500_000` → `"1.4 MiB"`.
 */
export function formatAttachmentBytes(bytes: number): string {
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < BINARY_UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const rendered = Number.isInteger(value) ? String(value) : value.toFixed(1);
  return `${rendered} ${BINARY_UNITS[unit]}`;
}
