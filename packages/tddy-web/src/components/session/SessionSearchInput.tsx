import React, { useEffect, useRef, useState } from "react";

export type SessionSearchInputProps = {
  /** Called after the user pauses typing (`debounceMs`). */
  onSearchQuery: (query: string) => void;
  debounceMs?: number;
  placeholder?: string;
};

/**
 * Sessions/workflow search field — debounced to avoid spamming the daemon RPC.
 * Wire `onSearchQuery` to ConnectionService `SearchSessions` via the app RPC layer.
 */
export function SessionSearchInput({
  onSearchQuery,
  debounceMs = 300,
  placeholder = "Search sessions…",
}: SessionSearchInputProps) {
  const [value, setValue] = useState("");
  const skipInitialDebounceRef = useRef(true);

  useEffect(() => {
    if (skipInitialDebounceRef.current) {
      skipInitialDebounceRef.current = false;
      return;
    }
    const t = window.setTimeout(() => {
      onSearchQuery(value);
    }, debounceMs);
    return () => {
      window.clearTimeout(t);
    };
  }, [value, debounceMs, onSearchQuery]);

  return (
    <input
      type="search"
      data-testid="session-search-input"
      placeholder={placeholder}
      value={value}
      aria-label="Search sessions"
      onChange={(e) => setValue(e.target.value)}
      className="border-input bg-background w-full max-w-md rounded-md border px-3 py-2 text-sm"
    />
  );
}
