# Insecure-origin constraints (tddy-web runs over plain http)

The daemon serves the tddy-web bundle over plain **`http://` on a LAN address** (`listen.web_port`),
and that is the normal way users reach it — phone or laptop pointing at the host's IP. Such an origin
is **not a secure context**: the browser exposes the standard globals but withholds every
secure-context-only member on them. Development on `localhost` hides this completely, because
`localhost` and `127.0.0.1` *are* secure contexts — so a feature can pass every local test and every
Cypress run and still throw on the first real device.

**Rule:** never call a secure-context-only API from tddy-web without a non-secure fallback. Notably
`crypto.randomUUID`, `navigator.clipboard`, `navigator.mediaDevices`, service workers, and
`navigator.geolocation`. `crypto.getRandomValues` **is** available on insecure origins — only the
UUID convenience wrapper is missing.

## Random ids — `lib/randomId.ts`

`randomUuid()` is the single audited entry point for client-generated ids:

1. native `crypto.randomUUID()` when it exists (secure origins),
2. else an RFC 4122 v4 built from `crypto.getRandomValues`,
3. else `Math.random` bytes.

It keeps the v4 UUID *shape* deliberately — ids like the terminal drop's `upload_id` become directory
names on the host and the daemon validates them as a single path segment, and existing daemon tests
and docs describe them as UUIDs. It degrades silently rather than feature-detecting at the call site:
these ids need to be collision-free, not cryptographically strong.

Units in `randomId.test.ts` cover all three branches by deleting `randomUUID` / removing the
`crypto` global, plus that the result is a safe single path segment.

## How this bit once

`useSessionFileUpload.uploadFiles` minted the per-drop `upload_id` with `crypto.randomUUID()` as its
very first statement. On a LAN origin the whole drop handler threw
(`TypeError: crypto.randomUUID is not a function`) before a single chunk was sent: no progress, no
error strip, no path typed — dragging a file simply did nothing. The mobile Attach button shares the
hook, so it failed identically. See
[terminal-file-upload.md](terminal-file-upload.md) and
[web-terminal.md § File drop upload](../../../docs/ft/web/web-terminal.md#file-drop-upload).
