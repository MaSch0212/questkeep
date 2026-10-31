import { afterEach, describe, expect, it, vi } from "vitest";
import {
  DEFAULT_VIEW_UI,
  clampWidth,
  collapsedKeys,
  createDebouncedSaver,
  toggleCollapsed,
} from "./viewUi";

describe("clampWidth", () => {
  it("keeps the width between the minimum and half the space", () => {
    expect(clampWidth(500, 320, 1600)).toBe(500);
    expect(clampWidth(200, 320, 1600)).toBe(320);
    expect(clampWidth(1000, 320, 1600)).toBe(800);
    expect(clampWidth(400.6, 320, 1600)).toBe(401);
  });

  it("never goes below the minimum, even when half the space is less", () => {
    expect(clampWidth(500, 320, 500)).toBe(320);
  });
});

describe("collapsed groups", () => {
  it("are kept per grouping field", () => {
    let state = toggleCollapsed(DEFAULT_VIEW_UI, "source", "steam");
    state = toggleCollapsed(state, "category", "rpg");
    expect(collapsedKeys(state, "source")).toEqual(["steam"]);
    expect(collapsedKeys(state, "category")).toEqual(["rpg"]);
    state = toggleCollapsed(state, "source", "steam");
    expect(collapsedKeys(state, "source")).toEqual([]);
    expect(collapsedKeys(state, "genre")).toEqual([]);
    expect(DEFAULT_VIEW_UI.collapsed).toEqual({});
  });
});

describe("createDebouncedSaver", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("saves the latest value per key once changes stop", () => {
    vi.useFakeTimers();
    const save = vi.fn<(key: string, value: number) => void>();
    const saver = createDebouncedSaver(save, 100);
    saver.schedule("a", 1);
    vi.advanceTimersByTime(60);
    saver.schedule("a", 2);
    saver.schedule("b", 3);
    vi.advanceTimersByTime(60);
    expect(save).not.toHaveBeenCalled();
    vi.advanceTimersByTime(60);
    expect(save.mock.calls).toEqual([
      ["a", 2],
      ["b", 3],
    ]);
  });

  it("flushes pending saves at once and cancels them", () => {
    vi.useFakeTimers();
    const save = vi.fn<(key: string, value: number) => void>();
    const saver = createDebouncedSaver(save, 100);
    saver.schedule("a", 1);
    saver.flush();
    expect(save).toHaveBeenCalledWith("a", 1);
    saver.schedule("b", 2);
    saver.cancel();
    vi.advanceTimersByTime(200);
    expect(save).toHaveBeenCalledTimes(1);
  });
});
