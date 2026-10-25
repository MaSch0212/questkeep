import { afterEach, describe, expect, it, vi } from "vitest";
import { loadViewMode, parseViewMode, saveViewMode, VIEW_MODE_KEY } from "./viewMode";

afterEach(() => {
  vi.restoreAllMocks();
  localStorage.clear();
});

describe("viewMode", () => {
  it("defaults to grid and round-trips list and details", () => {
    expect(loadViewMode()).toBe("grid");
    for (const mode of ["list", "details", "grid"] as const) {
      saveViewMode(mode);
      expect(localStorage.getItem(VIEW_MODE_KEY)).toBe(mode);
      expect(loadViewMode()).toBe(mode);
    }
  });

  it("parses stored values, keeping old ones working", () => {
    expect(parseViewMode("grid")).toBe("grid");
    expect(parseViewMode("list")).toBe("list");
    expect(parseViewMode("details")).toBe("details");
    expect(parseViewMode(null)).toBe("grid");
    expect(parseViewMode("tiles")).toBe("grid");
  });

  it("ignores garbage and unavailable storage", () => {
    localStorage.setItem(VIEW_MODE_KEY, "tiles");
    expect(loadViewMode()).toBe("grid");
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("denied");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("denied");
    });
    expect(loadViewMode()).toBe("grid");
    expect(() => saveViewMode("list")).not.toThrow();
  });
});
