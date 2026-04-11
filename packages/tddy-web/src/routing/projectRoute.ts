/**
 * Connection screen integration helpers for `/project/:encodedKey`.
 */

import { parseProjectRowKeyFromPathname } from "./appRoutes";

/** Parse project row key from the current pathname (single encoded segment after `/project/`). */
export function parseProjectRowKeyForConnectionScreen(pathname: string): string | null {
  const parsed = parseProjectRowKeyFromPathname(pathname);
  console.debug("[tddy][projectRoute] parseProjectRowKeyForConnectionScreen", { pathname, parsed });
  return parsed;
}
