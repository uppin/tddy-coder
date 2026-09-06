/**
 * Key continuity for host prompt keys — see `hostKeyPinning.ts`.
 *
 * The module is the only thing standing between the operator and an active key substitution on a
 * channel the trust model calls unauthenticated, so every verdict it can reach is pinned here,
 * including the ones a storage-blocked browser reaches.
 */

import { describe, expect, it } from "bun:test";
import { acceptChangedHostKey, checkHostKey } from "./hostKeyPinning";

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
