import { describe, expect, it } from "vitest";
import { fitToolbar, type FitItem } from "./toolbarFit";

const items: FitItem[] = [
  { id: "count", width: 60, menu: false },
  { id: "layout", width: 100, menu: true },
  { id: "save", width: 120, menu: true },
  { id: "sort", width: 30, menu: true },
];
const fit = (available: number) =>
  [...fitToolbar({ available, gap: 10, fixed: [150], items, moreWidth: 30 })].toSorted();

describe("fitToolbar", () => {
  it("keeps everything when it fits", () => {
    // 150 + 60 + 100 + 120 + 30 + 4 gaps
    expect(fit(500)).toEqual([]);
  });

  it("leaves out the count first, without a More button", () => {
    expect(fit(499)).toEqual(["count"]);
    expect(fit(430)).toEqual(["count"]);
  });

  it("moves items into the More menu in order, making room for its button", () => {
    // Without count and layout: 150 + 120 + 30 + more 30 + 3 gaps = 360.
    expect(fit(429)).toEqual(["count", "layout"]);
    expect(fit(360)).toEqual(["count", "layout"]);
    expect(fit(359)).toEqual(["count", "layout", "save"]);
    expect(fit(10)).toEqual(["count", "layout", "save", "sort"]);
  });
});
