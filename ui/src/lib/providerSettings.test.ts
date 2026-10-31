import { describe, expect, it } from "vitest";
import type { Settings } from "../bindings/Settings";
import type { StoreCredentials } from "../bindings/StoreCredentials";
import {
  credentialsSaved,
  providerFlag,
  providerText,
  withCredentials,
  withProviderOption,
} from "./providerSettings";

const base = {
  providers: { steam: { apiKey: "k", extraFlag: 7 }, gog: { other: 1 } },
} as unknown as Settings;

const credentials: StoreCredentials = {
  noun: "Key",
  offer: "Use a key",
  badge: "Key",
  detail: "Using a key",
  intro: "",
  link: null,
  fields: [
    { key: "apiKey", label: "Key", hint: null, secret: true, required: true, numeric: false },
    { key: "steamId", label: "Id", hint: null, secret: false, required: false, numeric: true },
  ],
};

describe("provider settings", () => {
  it("reads flags as false unless set to true", () => {
    expect(providerFlag(base, "gog", "launchThroughGalaxy")).toBe(false);
    expect(providerFlag(base, "epic", "launchThroughGalaxy")).toBe(false);
    const on = withProviderOption(base, "gog", "launchThroughGalaxy", true);
    expect(providerFlag(on, "gog", "launchThroughGalaxy")).toBe(true);
  });

  it("keeps other providers and keys", () => {
    const next = withProviderOption(base, "gog", "launchThroughGalaxy", true);
    expect(next.providers.steam).toEqual({ apiKey: "k", extraFlag: 7 });
    expect(next.providers.gog).toEqual({ other: 1, launchThroughGalaxy: true });
    expect(base.providers.gog).toEqual({ other: 1 });
  });
});

describe("credentials", () => {
  it("trims values, drops empty ones and keeps unknown keys", () => {
    const next = withCredentials(base, "steam", { apiKey: "  NEW  ", steamId: "" });
    expect(next.providers.steam).toEqual({ apiKey: "NEW", extraFlag: 7 });
    expect(withCredentials(base, "steam", { apiKey: "" }).providers.steam).toEqual({
      extraFlag: 7,
    });
    expect(next.providers.gog).toEqual({ other: 1 });
  });

  it("counts as saved when every required field has a value", () => {
    expect(credentialsSaved(base, "steam", credentials)).toBe(true);
    expect(credentialsSaved(base, "gog", credentials)).toBe(false);
    expect(credentialsSaved(null, "steam", credentials)).toBe(false);
    expect(providerText(base, "steam", "extraFlag")).toBe("");
  });
});
