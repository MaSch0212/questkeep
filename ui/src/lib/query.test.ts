import { describe, expect, it } from "vitest";
import {
  defaultDescending,
  defaultQuery,
  libraryView,
  mergeSources,
  normalizeSearch,
  scanSummaryText,
  toggleSource,
  withLibraryView,
} from "./query";

describe("library view", () => {
  it("maps views to query fields and back", () => {
    const q = defaultQuery();
    expect(libraryView(q)).toBe("all");
    const fav = withLibraryView(q, "favorites");
    expect(fav).toMatchObject({ favoritesOnly: true, hidden: "visible" });
    expect(libraryView(fav)).toBe("favorites");
    const hidden = withLibraryView(fav, "hidden");
    expect(hidden).toMatchObject({ favoritesOnly: false, hidden: "hidden" });
    expect(libraryView(hidden)).toBe("hidden");
  });
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
      { id: "manual", name: "Manual", canScan: false, canScanOwned: false, canUninstall: false },
      { id: "steam", name: "Steam", canScan: true, canScanOwned: true, canUninstall: true },
    ];
    expect(mergeSources(providers, new Set(["steam", "fake"]))).toEqual([
      { id: "manual", name: "Manual" },
      { id: "steam", name: "Steam" },
      { id: "fake", name: "Fake" },
    ]);
  });
  it("describes scan summaries", () => {
    expect(scanSummaryText({ added: 3, updated: 1, uninstalled: 0 })).toBe(
      "3 added, 1 updated, 0 uninstalled",
    );
  });
});
