import { describe, expect, it } from "vitest";
import type { AccountStatus } from "../bindings/AccountStatus";
import type { StoreCredentials } from "../bindings/StoreCredentials";
import { accountState, connectedSummary, storeRows } from "./accountState";

const account = (provider: string, over: Partial<AccountStatus> = {}): AccountStatus => ({
  provider,
  name: provider,
  signedIn: false,
  displayName: null,
  supportsCodeSignIn: false,
  codeUrl: null,
  codeHint: null,
  ...over,
});

const key: StoreCredentials = {
  noun: "Steam key",
  offer: "Use a key",
  badge: "API key",
  detail: "Using a Steam Web API key",
  intro: "",
  link: null,
  fields: [],
};

describe("accountState", () => {
  it("shows who is signed in", () => {
    expect(accountState(account("gog", { signedIn: true, displayName: "Ann" }))).toEqual({
      connection: "signedIn",
      badge: "Signed in",
      detail: "Signed in as Ann",
    });
    expect(accountState(account("gog", { signedIn: true })).detail).toBe(
      "Signed in as your account",
    );
  });

  it("counts saved credentials as connected, with the store's badge", () => {
    expect(accountState(account("steam"), key)).toEqual({
      connection: "credentials",
      badge: "API key",
      detail: "Using a Steam Web API key",
    });
    expect(accountState(account("steam", { signedIn: true }), key).connection).toBe("signedIn");
    expect(accountState(account("steam")).badge).toBe("Not signed in");
  });

  it("summarizes the connected stores", () => {
    const states = [
      accountState(account("gog", { signedIn: true })),
      accountState(account("steam"), key),
      accountState(account("ea")),
    ];
    expect(connectedSummary(states)).toBe("2 of 3 accounts connected");
    expect(connectedSummary(states.slice(2))).toBe("0 of 1 account connected");
  });
});

const provider = (id: string, canScan = true, installed = true) => ({
  id,
  name: id.toUpperCase(),
  icon: "",
  canScan,
  canScanOwned: false,
  canUninstall: false,
  installed,
  syncable: canScan && installed,
  addGameOptions: [],
  settings: { options: [], credentials: null },
});

describe("storeRows", () => {
  it("lists scanning stores in provider order with their accounts", () => {
    const rows = storeRows(
      [provider("manual", false), provider("gog"), provider("ubisoft"), provider("x", false)],
      [account("gog", { signedIn: true }), account("ea")],
    );
    expect(rows.map((r) => [r.id, r.account?.provider ?? null])).toEqual([
      ["gog", "gog"],
      ["ubisoft", null],
      ["ea", "ea"],
    ]);
    expect(rows[1]).toMatchObject({ name: "UBISOFT", canScan: true });
  });
});
