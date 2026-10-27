import type { AccountStatus } from "../bindings/AccountStatus";
import type { ProviderInfo } from "../bindings/ProviderInfo";

/** How a store account is connected: signed in, Steam's Web API key, or not at all. */
export type AccountConnection = "signedIn" | "apiKey" | "none";

export interface AccountState {
  connection: AccountConnection;
  /** Short status for the badge. */
  badge: string;
  /** The line under the store name. */
  detail: string;
}

/** What a row of the accounts list shows. `apiKeySaved` only matters for Steam. */
export function accountState(account: AccountStatus, apiKeySaved: boolean): AccountState {
  if (account.signedIn) {
    return {
      connection: "signedIn",
      badge: "Signed in",
      detail: `Signed in as ${account.displayName ?? "your account"}`,
    };
  }
  if (account.provider === "steam" && apiKeySaved) {
    return {
      connection: "apiKey",
      badge: "API key",
      detail: "Using a Steam Web API key",
    };
  }
  return { connection: "none", badge: "Not signed in", detail: "Owned games are not imported" };
}

/** "3 of 7 accounts connected". */
export function connectedSummary(states: AccountState[]): string {
  const connected = states.filter((s) => s.connection !== "none").length;
  return `${connected} of ${states.length} ${states.length === 1 ? "account" : "accounts"} connected`;
}

/** One row of the Stores settings: a store, and its account when it has a sign-in. */
export interface StoreRow {
  id: string;
  name: string;
  canScan: boolean;
  /** The store's client is on this PC. */
  installed: boolean;
  /** Syncing does something (client installed, or signed in). */
  syncable: boolean;
  account: AccountStatus | null;
}

/**
 * Every store with something to set up: the registered stores that scan games (not manually
 * added games) in provider order, then accounts of stores that are not registered.
 */
export function storeRows(providers: readonly ProviderInfo[], accounts: readonly AccountStatus[]) {
  const rows: StoreRow[] = providers
    .filter((p) => p.id !== "manual")
    .map((p) => ({
      id: p.id,
      name: p.name,
      canScan: p.canScan,
      installed: p.installed,
      syncable: p.syncable,
      account: accounts.find((a) => a.provider === p.id) ?? null,
    }))
    .filter((row) => row.canScan || row.account !== null);
  for (const account of accounts) {
    if (!rows.some((r) => r.id === account.provider)) {
      rows.push({
        id: account.provider,
        name: account.name,
        canScan: false,
        installed: false,
        syncable: false,
        account,
      });
    }
  }
  return rows;
}
