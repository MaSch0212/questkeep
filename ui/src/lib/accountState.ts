import type { AccountStatus } from "../bindings/AccountStatus";
import type { ProviderInfo } from "../bindings/ProviderInfo";
import type { StoreCredentials } from "../bindings/StoreCredentials";
import type { StoreSettings } from "../bindings/StoreSettings";
import { translator } from "../i18n";
import { settingsAccounts } from "../i18n/settingsAccounts";

const tr = translator(settingsAccounts);

/** How a store account is connected: signed in, saved credentials (Steam's Web API key), or not at all. */
export type AccountConnection = "signedIn" | "credentials" | "none";

export interface AccountState {
  connection: AccountConnection;
  /** Short status for the badge. */
  badge: string;
  /** The line under the store name. */
  detail: string;
}

/** What a row of the accounts list shows. `saved` are the store's credentials when they are saved. */
export function accountState(
  account: AccountStatus,
  saved: StoreCredentials | null = null,
): AccountState {
  if (account.signedIn) {
    return {
      connection: "signedIn",
      badge: tr("signedIn"),
      detail: tr("signedInAs", { name: account.displayName ?? tr("yourAccount") }),
    };
  }
  if (saved !== null) {
    return { connection: "credentials", badge: saved.badge, detail: saved.detail };
  }
  return { connection: "none", badge: tr("notSignedIn"), detail: tr("ownedNotImported") };
}

/** "3 of 7 accounts connected". */
export function connectedSummary(states: AccountState[]): string {
  const connected = states.filter((s) => s.connection !== "none").length;
  return tr("connectedSummary", { connected, total: states.length });
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
  /** The store's own settings. */
  settings: StoreSettings;
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
      settings: p.settings,
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
        settings: { options: [], credentials: null },
      });
    }
  }
  return rows;
}
