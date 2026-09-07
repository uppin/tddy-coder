/**
 * Key continuity for host prompt keys — see `hostKeyPinning.ts`.
 *
 * The module is the only thing standing between the operator and an active key substitution on a
 * channel the trust model calls unauthenticated, so every verdict it can reach is pinned here,
 * including the ones a storage-blocked browser reaches.
 */

import { afterEach, describe, expect, it } from "bun:test";
import { createHash } from "node:crypto";
import { acceptChangedHostKey, checkHostKey, verifyHostKey } from "./hostKeyPinning";

// ---------------------------------------------------------------------------
// Fixtures — two hosts and three key fingerprints, in the shape the daemon publishes
// ---------------------------------------------------------------------------

const WORKSHOP_MINI = "workshop-mini";
const OFFICE_TOWER = "office-tower";

const ORIGINAL_KEY = "SHA256:47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU";
const SUBSTITUTED_KEY = "SHA256:LPJNul+wow4m6DsqxbninhsWHlwfp0JecwQzYpOLmCQ";
const ROTATED_KEY = "SHA256:n4bQgYhMfWWaL+qgxVrQFaO/TxsrC4Is0V1sFbDwCgg";

/** A key entry another part of the app owns, present to prove pins do not collide with it. */
const UNRELATED_TDDY_ENTRY = "tddy:debug:configBase";

// ---------------------------------------------------------------------------
// Storage doubles
// ---------------------------------------------------------------------------

/** An in-memory `Storage` whose contents a test can read back. */
interface FakeStorage extends Storage {
  /** Everything currently stored, as a plain object. */
  snapshot(): Record<string, string>;
}

function fakeStorageOver(
  store: Map<string, string>,
  overrides: Partial<Pick<Storage, "getItem" | "setItem">> = {},
): FakeStorage {
  return {
    get length() {
      return store.size;
    },
    clear() {
      store.clear();
    },
    key(index: number) {
      return [...store.keys()][index] ?? null;
    },
    removeItem(key: string) {
      store.delete(key);
    },
    getItem: overrides.getItem ?? ((key: string) => store.get(key) ?? null),
    setItem: overrides.setItem ?? ((key: string, value: string) => void store.set(key, value)),
    snapshot: () => Object.fromEntries(store),
  };
}

/** A working browser store, optionally pre-seeded with entries other code owns. */
function aWorkingStorage(seed: Record<string, string> = {}): FakeStorage {
  return fakeStorageOver(new Map(Object.entries(seed)));
}

/** A store that throws on every read, the way a storage-blocked browser does. */
function aStorageThatRefusesReads(): FakeStorage {
  return fakeStorageOver(new Map(), {
    getItem: () => {
      throw new DOMException("The operation is insecure.", "SecurityError");
    },
  });
}

/** A store that reads empty but throws on every write, the way an exhausted quota does. */
function aStorageThatRefusesWrites(): FakeStorage {
  return fakeStorageOver(new Map(), {
    setItem: () => {
      throw new DOMException("The quota has been exceeded.", "QuotaExceededError");
    },
  });
}

/** Installs `storage` as `window.localStorage` for the duration of `fn`, then restores the global. */
function withBrowserStorage(storage: FakeStorage, fn: () => void): void {
  const global = globalThis as typeof globalThis & { window?: Window };
  const previousWindow = global.window;
  global.window = { ...previousWindow, localStorage: storage } as unknown as Window;
  try {
    fn();
  } finally {
    if (previousWindow !== undefined) global.window = previousWindow;
    else delete global.window;
  }
}

/** {@link withBrowserStorage} for a check that has to await a digest before it can conclude. */
async function withBrowserStorageAsync(
  storage: FakeStorage,
  fn: () => Promise<void>,
): Promise<void> {
  const global = globalThis as typeof globalThis & { window?: Window };
  const previousWindow = global.window;
  global.window = { ...previousWindow, localStorage: storage } as unknown as Window;
  try {
    await fn();
  } finally {
    if (previousWindow !== undefined) global.window = previousWindow;
    else delete global.window;
  }
}

// ---------------------------------------------------------------------------
// Trust on first use
// ---------------------------------------------------------------------------

describe("checkHostKey", () => {
  it("pins the key and reports a first sighting for a host never seen before", () => {
    // Given
    const storage = aWorkingStorage();

    withBrowserStorage(storage, () => {
      // When
      const verdict = checkHostKey(WORKSHOP_MINI, ORIGINAL_KEY);

      // Then
      expect(verdict).toEqual({ kind: "pinned-now" });
      expect(Object.values(storage.snapshot())).toEqual([ORIGINAL_KEY]);
    });
  });

  it("reports the key unchanged when the host presents the fingerprint that was pinned", () => {
    // Given
    const storage = aWorkingStorage();

    withBrowserStorage(storage, () => {
      checkHostKey(WORKSHOP_MINI, ORIGINAL_KEY);

      // When
      const verdict = checkHostKey(WORKSHOP_MINI, ORIGINAL_KEY);

      // Then
      expect(verdict).toEqual({ kind: "unchanged" });
    });
  });

  it("reports the key changed and carries the pinned fingerprint when the host presents another key", () => {
    // Given
    const storage = aWorkingStorage();

    withBrowserStorage(storage, () => {
      checkHostKey(WORKSHOP_MINI, ORIGINAL_KEY);

      // When
      const verdict = checkHostKey(WORKSHOP_MINI, SUBSTITUTED_KEY);

      // Then
      expect(verdict).toEqual({ kind: "changed", pinnedFingerprint: ORIGINAL_KEY });
    });
  });

  it("keeps one pin per host, so pinning one host does not answer for another", () => {
    // Given
    const storage = aWorkingStorage();

    withBrowserStorage(storage, () => {
      checkHostKey(WORKSHOP_MINI, ORIGINAL_KEY);

      // When
      const otherHostVerdict = checkHostKey(OFFICE_TOWER, SUBSTITUTED_KEY);

      // Then
      expect(otherHostVerdict).toEqual({ kind: "pinned-now" });
      expect(checkHostKey(WORKSHOP_MINI, ORIGINAL_KEY)).toEqual({ kind: "unchanged" });
    });
  });

  // -------------------------------------------------------------------------
  // No key material, and no usable storage — the sightings that cannot conclude
  // -------------------------------------------------------------------------

  it("draws no continuity conclusion for a host that presents an empty fingerprint", () => {
    // Given
    const storage = aWorkingStorage();

    withBrowserStorage(storage, () => {
      // When
      const verdict = checkHostKey(WORKSHOP_MINI, "");

      // Then — an absent key is not a key, so it must not consume the first-use trust slot
      expect(verdict).toEqual({ kind: "unverified" });
      expect(storage.snapshot()).toEqual({});
    });
  });

  it("leaves an established pin intact when the host presents an empty fingerprint", () => {
    // Given
    const storage = aWorkingStorage();

    withBrowserStorage(storage, () => {
      checkHostKey(WORKSHOP_MINI, ORIGINAL_KEY);
      checkHostKey(WORKSHOP_MINI, "");

      // When
      const verdict = checkHostKey(WORKSHOP_MINI, ORIGINAL_KEY);

      // Then
      expect(verdict).toEqual({ kind: "unchanged" });
    });
  });

  it("draws no continuity conclusion when localStorage refuses to be read", () => {
    // Given
    const storage = aStorageThatRefusesReads();

    withBrowserStorage(storage, () => {
      // When
      const verdict = checkHostKey(WORKSHOP_MINI, ORIGINAL_KEY);

      // Then — no pin can be read, so claiming the key is now pinned would be a false reassurance
      expect(verdict).toEqual({ kind: "unverified" });
    });
  });

  it("draws no continuity conclusion when localStorage refuses the write", () => {
    // Given
    const storage = aStorageThatRefusesWrites();

    withBrowserStorage(storage, () => {
      // When
      const verdict = checkHostKey(WORKSHOP_MINI, ORIGINAL_KEY);

      // Then — nothing was recorded, so this sighting pinned nothing to compare against later
      expect(verdict).toEqual({ kind: "unverified" });
    });
  });

  // -------------------------------------------------------------------------
  // Storage namespace
  // -------------------------------------------------------------------------

  it("records the pin under a key namespaced to host key pinning", () => {
    // Given
    const storage = aWorkingStorage();

    withBrowserStorage(storage, () => {
      // When
      checkHostKey(WORKSHOP_MINI, ORIGINAL_KEY);

      // Then
      expect(Object.keys(storage.snapshot())).toEqual(["tddy.hostKeyPin.workshop-mini"]);
    });
  });

  it("leaves localStorage entries owned by other tddy code untouched", () => {
    // Given
    const storage = aWorkingStorage({ [UNRELATED_TDDY_ENTRY]: "tddy:term:*" });

    withBrowserStorage(storage, () => {
      // When
      checkHostKey(WORKSHOP_MINI, ORIGINAL_KEY);

      // Then
      expect(storage.snapshot()).toEqual({
        [UNRELATED_TDDY_ENTRY]: "tddy:term:*",
        "tddy.hostKeyPin.workshop-mini": ORIGINAL_KEY,
      });
    });
  });
});

// ---------------------------------------------------------------------------
// Operator-accepted key changes
// ---------------------------------------------------------------------------

describe("acceptChangedHostKey", () => {
  it("makes the accepted fingerprint the pin the next sighting is compared against", () => {
    // Given
    const storage = aWorkingStorage();

    withBrowserStorage(storage, () => {
      checkHostKey(WORKSHOP_MINI, ORIGINAL_KEY);
      checkHostKey(WORKSHOP_MINI, ROTATED_KEY);

      // When
      acceptChangedHostKey(WORKSHOP_MINI, ROTATED_KEY);

      // Then
      expect(checkHostKey(WORKSHOP_MINI, ROTATED_KEY)).toEqual({ kind: "unchanged" });
    });
  });

  it("still flags the previously pinned key as a change once a new key is accepted", () => {
    // Given
    const storage = aWorkingStorage();

    withBrowserStorage(storage, () => {
      checkHostKey(WORKSHOP_MINI, ORIGINAL_KEY);
      acceptChangedHostKey(WORKSHOP_MINI, ROTATED_KEY);

      // When
      const verdict = checkHostKey(WORKSHOP_MINI, ORIGINAL_KEY);

      // Then
      expect(verdict).toEqual({ kind: "changed", pinnedFingerprint: ROTATED_KEY });
    });
  });

  it("does not propagate a localStorage write failure to the caller", () => {
    // Given
    const storage = aStorageThatRefusesWrites();

    withBrowserStorage(storage, () => {
      // When / Then
      expect(() => acceptChangedHostKey(WORKSHOP_MINI, ROTATED_KEY)).not.toThrow();
    });
  });
});

// ---------------------------------------------------------------------------
// Binding the pin to the key that encrypts, not to the string beside it
// ---------------------------------------------------------------------------

/**
 * A prompt carries a key *and* a fingerprint string, and only one of them encrypts anything. Pinning
 * the string would leave the pin decorative: an active peer replays the genuine, non-secret
 * fingerprint beside its own key, the check says "unchanged", the operator recognises the
 * fingerprint they verified out of band, and the passphrase is encrypted to the peer.
 *
 * `verifyHostKey` derives the fingerprint from the key's own bytes and pins *that*, so the recorded
 * value and the encrypting key are the same fact.
 */

const realCrypto = globalThis.crypto;

function setCrypto(value: unknown) {
  Object.defineProperty(globalThis, "crypto", { value, configurable: true, writable: true });
}

/** Stands in for a published SPKI DER — the check hashes these bytes and never imports them. */
const A_HOSTS_OWN_KEY = new Uint8Array([0x30, 0x82, 0x01, 0x22, 0x01]);
const A_SUBSTITUTED_KEY = new Uint8Array([0x30, 0x82, 0x01, 0x22, 0x02]);
const A_ROTATED_KEY = new Uint8Array([0x30, 0x82, 0x01, 0x22, 0x03]);

/** What `host_keypair.rs` publishes for these bytes, computed here rather than through the module. */
function fingerprintTheDaemonWouldPublish(spkiDer: Uint8Array): string {
  return `SHA256:${createHash("sha256").update(spkiDer).digest("base64").replace(/=+$/, "")}`;
}

describe("verifyHostKey", () => {
  afterEach(() => {
    setCrypto(realCrypto);
  });

  it("pins the fingerprint derived from the key the prompt carried", async () => {
    // Given a host seen for the first time, advertising its key honestly
    const storage = aWorkingStorage();
    const honest = fingerprintTheDaemonWouldPublish(A_HOSTS_OWN_KEY);

    await withBrowserStorageAsync(storage, async () => {
      // When the prompt is checked
      const check = await verifyHostKey(WORKSHOP_MINI, A_HOSTS_OWN_KEY, honest);

      // Then the sighting is recorded under the digest of the key itself — the value later
      // sightings are compared against is the one that encrypts
      expect(check).toEqual({ verdict: { kind: "pinned-now" }, fingerprint: honest });
      expect(Object.values(storage.snapshot())).toEqual([honest]);
    });
  });

  it("blocks a prompt whose advertised fingerprint is not the fingerprint of its key", async () => {
    // Given a prompt carrying one key while claiming another key's fingerprint
    const storage = aWorkingStorage();
    const claimed = fingerprintTheDaemonWouldPublish(A_HOSTS_OWN_KEY);
    const actual = fingerprintTheDaemonWouldPublish(A_SUBSTITUTED_KEY);

    await withBrowserStorageAsync(storage, async () => {
      // When it is checked
      const check = await verifyHostKey(WORKSHOP_MINI, A_SUBSTITUTED_KEY, claimed);

      // Then the two are reported as disagreeing, and nothing is pinned: a sighting that cannot say
      // which key it saw must not spend this host's one first-use trust slot
      expect(check.verdict).toEqual({
        kind: "mismatched",
        advertisedFingerprint: claimed,
        derivedFingerprint: actual,
      });
      expect(storage.snapshot()).toEqual({});
    });
  });

  it("catches a genuine fingerprint replayed beside another key", async () => {
    // Given this host was seen once, honestly, and its key pinned
    const storage = aWorkingStorage();
    const honest = fingerprintTheDaemonWouldPublish(A_HOSTS_OWN_KEY);

    await withBrowserStorageAsync(storage, async () => {
      await verifyHostKey(WORKSHOP_MINI, A_HOSTS_OWN_KEY, honest);

      // When a peer replays that same fingerprint — public, non-secret — beside its own key
      const check = await verifyHostKey(WORKSHOP_MINI, A_SUBSTITUTED_KEY, honest);

      // Then the substitution is caught, rather than reading as the key the operator verified
      expect(check.verdict).toEqual({
        kind: "mismatched",
        advertisedFingerprint: honest,
        derivedFingerprint: fingerprintTheDaemonWouldPublish(A_SUBSTITUTED_KEY),
      });
      // And the pin still records the key that was genuinely seen
      expect(Object.values(storage.snapshot())).toEqual([honest]);
    });
  });

  it("reports the key unchanged when the pinned key comes back", async () => {
    // Given a host whose key was pinned on a first sighting
    const storage = aWorkingStorage();
    const honest = fingerprintTheDaemonWouldPublish(A_HOSTS_OWN_KEY);

    await withBrowserStorageAsync(storage, async () => {
      await verifyHostKey(WORKSHOP_MINI, A_HOSTS_OWN_KEY, honest);

      // When the same key raises another prompt
      const check = await verifyHostKey(WORKSHOP_MINI, A_HOSTS_OWN_KEY, honest);

      // Then nothing is remarked on
      expect(check).toEqual({ verdict: { kind: "unchanged" }, fingerprint: honest });
    });
  });

  it("reports the key changed when a different key arrives honestly advertised", async () => {
    // Given a host whose key was pinned, that has since regenerated its keypair
    const storage = aWorkingStorage();
    const original = fingerprintTheDaemonWouldPublish(A_HOSTS_OWN_KEY);
    const rotated = fingerprintTheDaemonWouldPublish(A_ROTATED_KEY);

    await withBrowserStorageAsync(storage, async () => {
      await verifyHostKey(WORKSHOP_MINI, A_HOSTS_OWN_KEY, original);

      // When the new key raises a prompt, advertising itself truthfully
      const check = await verifyHostKey(WORKSHOP_MINI, A_ROTATED_KEY, rotated);

      // Then it is a rotation to accept or refuse, not a key claiming to be one it is not
      expect(check).toEqual({
        verdict: { kind: "changed", pinnedFingerprint: original },
        fingerprint: rotated,
      });
    });
  });

  it("draws no conclusion for a prompt that carries no key at all", async () => {
    // Given a prompt with no key material to hash
    const storage = aWorkingStorage();

    await withBrowserStorageAsync(storage, async () => {
      // When it is checked
      const check = await verifyHostKey(WORKSHOP_MINI, new Uint8Array(), "");

      // Then no first sighting is manufactured out of nothing
      expect(check).toEqual({ verdict: { kind: "unverified" }, fingerprint: null });
      expect(storage.snapshot()).toEqual({});
    });
  });

  it("cannot check a key on an origin that exposes no crypto.subtle, and says why", async () => {
    // Given the origin the daemon serves this bundle on: plain http, where `subtle` is withheld
    const storage = aWorkingStorage();
    const honest = fingerprintTheDaemonWouldPublish(A_HOSTS_OWN_KEY);
    setCrypto({ getRandomValues: realCrypto.getRandomValues.bind(realCrypto) });

    await withBrowserStorageAsync(storage, async () => {
      // When a prompt is checked there
      const check = await verifyHostKey(WORKSHOP_MINI, A_HOSTS_OWN_KEY, honest);

      // Then the check reports that it could not run, with the reason — and pins nothing, because a
      // fingerprint it could not derive is a fingerprint it cannot stand behind
      expect(check.fingerprint).toBeNull();
      expect(check.verdict.kind).toBe("underivable");
      expect(check.verdict).toHaveProperty("reason", expect.stringMatching(/secure context/i));
      expect(storage.snapshot()).toEqual({});
    });
  });
});
