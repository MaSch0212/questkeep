import { describe, expect, it } from "vitest";
import type { RegistryPluginView } from "../bindings/RegistryPluginView";
import { makeRegistryPlugin, makeVersion } from "../test-utils";
import {
  compareVersions,
  filterRegistry,
  hasHiddenVersions,
  listedVersions,
  NO_FILTER,
  registryKinds,
  sortRegistry,
  versionAction,
  versionBlock,
} from "./pluginRegistry";

describe("compareVersions", () => {
  it("compares numbers, not text", () => {
    expect(compareVersions("1.10.0", "1.9.0")).toBe(1);
    expect(compareVersions("1.0.0", "1.0.0")).toBe(0);
    expect(compareVersions("0.9.9", "1.0.0")).toBe(-1);
    expect(compareVersions("1.0", "1.0.0")).toBe(0);
  });

  it("puts a pre-release below its release and ignores build metadata", () => {
    expect(compareVersions("1.0.0-beta.1", "1.0.0")).toBe(-1);
    expect(compareVersions("1.0.0", "1.0.0-rc.1")).toBe(1);
    expect(compareVersions("1.0.0-beta.2", "1.0.0-beta.10")).toBe(-1);
    expect(compareVersions("1.0.0+5", "1.0.0")).toBe(0);
  });
});

const ids = (list: RegistryPluginView[]) => list.map((p) => p.id);

describe("filterRegistry", () => {
  const theme = makeRegistryPlugin();
  const store = makeRegistryPlugin({
    id: "bs",
    name: "BSManager",
    summary: "Beat Saber maps",
    author: "Grace",
    tags: ["store", "beat-saber"],
    kinds: ["store"],
    installed: { version: "1.0.0", pinned: false, fromRegistry: true },
  });
  const both = makeRegistryPlugin({
    id: "up",
    name: "Updater",
    kinds: ["extension", "theme"],
    installed: { version: "1.0.0", pinned: false, fromRegistry: true },
    updateAvailable: true,
  });
  const all = [theme, store, both];

  it("matches every word in name, summary, author and tags", () => {
    expect(ids(filterRegistry(all, { ...NO_FILTER, query: "beat saber" }))).toEqual(["bs"]);
    expect(ids(filterRegistry(all, { ...NO_FILTER, query: "GRACE" }))).toEqual(["bs"]);
    expect(ids(filterRegistry(all, { ...NO_FILTER, query: "warm ada" }))).toEqual([
      "ember-pack",
      "up",
    ]);
    expect(filterRegistry(all, { ...NO_FILTER, query: "zzz" })).toEqual([]);
    expect(filterRegistry(all, NO_FILTER)).toHaveLength(3);
  });

  it("filters by kind (any of the chosen), installed and updates", () => {
    expect(ids(filterRegistry(all, { ...NO_FILTER, kinds: new Set(["store"]) }))).toEqual(["bs"]);
    expect(
      ids(filterRegistry(all, { ...NO_FILTER, kinds: new Set(["extension", "store"]) })),
    ).toEqual(["bs", "up"]);
    expect(ids(filterRegistry(all, { ...NO_FILTER, installed: true }))).toEqual(["bs", "up"]);
    expect(ids(filterRegistry(all, { ...NO_FILTER, updates: true }))).toEqual(["up"]);
    expect(
      ids(filterRegistry(all, { ...NO_FILTER, installed: true, kinds: new Set(["theme"]) })),
    ).toEqual(["up"]);
  });

  it("lists known kinds in display order", () => {
    expect(registryKinds(makeRegistryPlugin({ kinds: ["theme", "weird", "store"] }))).toEqual([
      "store",
      "theme",
    ]);
  });
});

describe("sortRegistry", () => {
  it("puts plugins with an update first, then sorts by name", () => {
    const sorted = sortRegistry([
      makeRegistryPlugin({ id: "c", name: "Charlie" }),
      makeRegistryPlugin({ id: "z", name: "Zed", updateAvailable: true }),
      makeRegistryPlugin({ id: "a", name: "alpha" }),
    ]);
    expect(sorted.map((p) => p.id)).toEqual(["z", "a", "c"]);
  });
});

const installedAt = (version: string, fromRegistry = true) =>
  makeRegistryPlugin({ installed: { version, pinned: false, fromRegistry } });

describe("versionAction", () => {
  it("installs when nothing from the registry is installed", () => {
    expect(versionAction(makeRegistryPlugin(), "1.0.0")).toBe("install");
    expect(versionAction(installedAt("1.0.0", false), "1.1.0")).toBe("install");
  });

  it("updates, downgrades or reinstalls relative to the installed version", () => {
    expect(versionAction(installedAt("1.0.0"), "1.1.0")).toBe("update");
    expect(versionAction(installedAt("1.1.0"), "1.0.0")).toBe("downgrade");
    expect(versionAction(installedAt("1.1.0"), "1.1.0")).toBe("reinstall");
  });
});

const pick = <T>(list: readonly T[], index: number): T => {
  const item = list[index];
  if (item === undefined) throw new Error(`no item ${index}`);
  return item;
};

describe("versions", () => {
  const plugin = makeRegistryPlugin({
    versions: [
      makeVersion("0.9.0", { yanked: true }),
      makeVersion("1.0.0"),
      makeVersion("2.0.0", { compatible: false, minApp: "99.0.0" }),
    ],
  });

  it("names why a version is blocked", () => {
    expect(versionBlock(pick(plugin.versions, 0))).toBe("yanked");
    expect(versionBlock(pick(plugin.versions, 1))).toBeNull();
    expect(versionBlock(pick(plugin.versions, 2))).toBe("incompatible");
    expect(versionBlock(makeVersion("1.0.0", { yanked: true, compatible: false }))).toBe("yanked");
  });

  it("lists installable versions newest first, or all of them", () => {
    expect(listedVersions(plugin, false).map((v) => v.version)).toEqual(["1.0.0"]);
    expect(listedVersions(plugin, true).map((v) => v.version)).toEqual(["2.0.0", "1.0.0", "0.9.0"]);
    expect(hasHiddenVersions(plugin)).toBe(true);
    expect(hasHiddenVersions(makeRegistryPlugin())).toBe(false);
  });
});
