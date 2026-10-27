import { describe, expect, it } from "vitest";
import type { AccountStatus } from "../bindings/AccountStatus";
import { accountState, connectedSummary, storeRows } from "./accountState";

const account = (provider: string, over: Partial<AccountStatus> = {}): AccountStatus => ({
  provider,
  name: provider,
  signedIn: false,
  displayName: null,
  supportsCodeSignIn: false,
  codeUrl: null,
  ...over,
});

describe("accountState", () => {
  it("shows who is signed in", () => {
    expect(accountState(account("gog", { signedIn: true, displayName: "Ann" }), false)).toEqual({
      connection: "signedIn",
      badge: "Signed in",
      detail: "Signed in as Ann",
    });
    expect(accountState(account("gog", { signedIn: true }), false).detail).toBe(
      "Signed in as your account",
    );
  });

  it("counts a saved Steam key as connected, only for Steam", () => {
    expect(accountState(account("steam"), true).connection).toBe("apiKey");
    expect(accountState(account("steam", { signedIn: true }), true).connection).toBe("signedIn");
    expect(accountState(account("gog"), true).connection).toBe("none");
    expect(accountState(account("steam"), false).badge).toBe("Not signed in");
  });

  it("summarizes the connected stores", () => {
    const states = [
      accountState(account("gog", { signedIn: true }), false),
      accountState(account("steam"), true),
      accountState(account("ea"), false),
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
