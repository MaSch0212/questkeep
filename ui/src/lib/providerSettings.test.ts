import { describe, expect, it } from "vitest";
import type { Settings } from "../bindings/Settings";
import { GOG_LAUNCH_THROUGH_GALAXY, providerFlag, withProviderOption } from "./providerSettings";

const base = {
  providers: { steam: { apiKey: "k" }, gog: { other: 1 } },
} as unknown as Settings;

describe("provider settings", () => {
  it("reads flags as false unless set to true", () => {
    expect(providerFlag(base, "gog", GOG_LAUNCH_THROUGH_GALAXY)).toBe(false);
    expect(providerFlag(base, "epic", GOG_LAUNCH_THROUGH_GALAXY)).toBe(false);
    const on = withProviderOption(base, "gog", GOG_LAUNCH_THROUGH_GALAXY, true);
    expect(providerFlag(on, "gog", GOG_LAUNCH_THROUGH_GALAXY)).toBe(true);
  });

  it("keeps other providers and keys", () => {
    const next = withProviderOption(base, "gog", GOG_LAUNCH_THROUGH_GALAXY, true);
    expect(next.providers.steam).toEqual({ apiKey: "k" });
    expect(next.providers.gog).toEqual({ other: 1, launchThroughGalaxy: true });
    expect(base.providers.gog).toEqual({ other: 1 });
  });
});
