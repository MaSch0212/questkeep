import { describe, expect, it } from "vitest";
import type { ProviderInfo } from "../bindings/ProviderInfo";
import {
  defaultDescending,
  mergeSources,
  normalizeSearch,
  scanSummaryText,
  toggleSource,
} from "./query";

const store = (id: string, installed: boolean, syncable: boolean): ProviderInfo => ({
  id,
  name: id,
  icon: "",
  canScan: true,
  canScanOwned: false,
  canUninstall: false,
  installed,
  syncable,
  addGameOptions: [],
});

describe("helpers", () => {
  it("picks default sort direction", () => {
    expect(defaultDescending("name")).toBe(false);
    expect(defaultDescending("playtime")).toBe(true);
  });
  it("toggles sources", () => {
    expect(toggleSource([], "steam")).toEqual(["steam"]);
    expect(toggleSource(["steam", "fake"], "steam")).toEqual(["fake"]);
  });
  it("normalizes search", () => {
    expect(normalizeSearch("  ")).toBeNull();
    expect(normalizeSearch(" hi ")).toBe("hi");
  });
  it("merges providers with seen sources", () => {
    const providers = [
      {
        id: "manual",
        name: "Manual",
        icon: "<svg></svg>",
        canScan: false,
        canScanOwned: false,
        canUninstall: false,
        installed: true,
        syncable: false,
        addGameOptions: [],
      },
      {
        id: "steam",
        name: "Steam",
        icon: "<svg></svg>",
        canScan: true,
        canScanOwned: true,
        canUninstall: true,
        installed: true,
        syncable: true,
        addGameOptions: [],
      },
    ];
    expect(mergeSources(providers, new Set(["steam", "fake"]))).toEqual([
      { id: "manual", name: "Manual", canScan: false },
      { id: "steam", name: "Steam", canScan: true },
      { id: "fake", name: "Fake", canScan: false },
    ]);
  });
  it("leaves out stores that are neither installed, synced nor in the library", () => {
    const providers = [
      store("prism", false, false),
      store("gog", false, true),
      store("steam", true, true),
      store("ea", false, false),
    ];
    expect(mergeSources(providers, new Set(["ea"])).map((s) => s.id)).toEqual([
      "gog",
      "steam",
      "ea",
    ]);
  });
  it("describes scan summaries", () => {
    expect(scanSummaryText({ added: 3, updated: 1, uninstalled: 0 })).toBe(
      "3 added, 1 updated, 0 uninstalled",
    );
  });
});
