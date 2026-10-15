/**
 * One account a project can be assigned, flattened from `ListAccountsResponse`. An account id is
 * unique only **within** its provider, so both halves travel together everywhere.
 */
export interface AssignableAccount {
  provider: string;
  accountId: string;
  label: string;
}

export interface AssignmentRow {
  provider: string;
  /** "" when the project holds no assignment at this provider. */
  assignedAccountId: string;
  /** The assigned account is not among this host's vault accounts for the provider. */
  assignedUnavailable: boolean;
  options: AssignableAccount[];
}

/**
 * One row per provider the vault holds an account at, in vault order, plus a row for any provider
 * the project is assigned at whose account this host does not know — that assignment is still real
 * and must be visible, as "unavailable", never silently shown as unassigned.
 */
export function assignmentRowsFor(
  assigned: { provider: string; accountId: string }[],
  vault: AssignableAccount[],
): AssignmentRow[] {
  const providers = [...new Set([...vault.map((a) => a.provider), ...assigned.map((a) => a.provider)])];
  return providers.map((provider) => {
    const options = vault.filter((a) => a.provider === provider);
    const assignedAccountId = assigned.find((a) => a.provider === provider)?.accountId ?? "";
    return {
      provider,
      assignedAccountId,
      assignedUnavailable:
        assignedAccountId !== "" && !options.some((a) => a.accountId === assignedAccountId),
      options,
    };
  });
}

/** The whole assignment set with one provider's entry replaced, or removed when `accountId` is "". */
export function withAssignment(
  assigned: { provider: string; accountId: string }[],
  provider: string,
  accountId: string,
): { provider: string; accountId: string }[] {
  const others = assigned.filter((a) => a.provider !== provider);
  if (accountId === "") return others;
  const existing = assigned.some((a) => a.provider === provider);
  return existing
    ? assigned.map((a) => (a.provider === provider ? { provider, accountId } : a))
    : [...others, { provider, accountId }];
}
