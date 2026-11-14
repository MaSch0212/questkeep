import { describe, expect, it } from "vitest";
import { makePlugin } from "../test-utils";
import { hasPermissions, pluginKinds, pluginStatus, sortPlugins } from "./plugins";

describe("plugins", () => {
  it("lists what a plugin brings", () => {
    expect(pluginKinds(makePlugin())).toEqual(["store"]);
    expect(pluginKinds(makePlugin({ providers: [], hasExtension: true, themes: 2 }))).toEqual([
      "extension",
      "theme",
    ]);
    expect(pluginKinds(makePlugin({ providers: [] }))).toEqual([]);
  });

  it("tells loaded, disabled and failed plugins apart", () => {
    expect(pluginStatus(makePlugin())).toBe("loaded");
    expect(pluginStatus(makePlugin({ enabled: false, loaded: false }))).toBe("disabled");
    expect(pluginStatus(makePlugin({ enabled: false, loaded: false, error: "x" }))).toBe(
      "disabled",
    );
    expect(pluginStatus(makePlugin({ loaded: false, error: "bad wasm" }))).toBe("failed");
    expect(pluginStatus(makePlugin({ loaded: false }))).toBe("failed");
  });

  it("sorts by name, ignoring case", () => {
    const list = [
      makePlugin({ id: "b", name: "beta" }),
      makePlugin({ id: "a", name: "Alpha" }),
      makePlugin({ id: "c", name: "Alpha" }),
    ];
    expect(sortPlugins(list).map((p) => p.id)).toEqual(["a", "c", "b"]);
    expect(list[0]?.id).toBe("b");
  });

  it("knows whether a plugin asks for extra access", () => {
    expect(hasPermissions(makePlugin())).toBe(false);
    expect(hasPermissions(makePlugin({ permissions: { read: [], http: ["a.com"] } }))).toBe(true);
  });
});
