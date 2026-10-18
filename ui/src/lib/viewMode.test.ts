import { afterEach, describe, expect, it, vi } from "vitest";
import { loadViewMode, saveViewMode, VIEW_MODE_KEY } from "./viewMode";

afterEach(() => {
  vi.restoreAllMocks();
  localStorage.clear();
});

describe("viewMode", () => {
  it("defaults to grid and round-trips list", () => {
    expect(loadViewMode()).toBe("grid");
    saveViewMode("list");
    expect(localStorage.getItem(VIEW_MODE_KEY)).toBe("list");
    expect(loadViewMode()).toBe("list");
    saveViewMode("grid");
    expect(loadViewMode()).toBe("grid");
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
