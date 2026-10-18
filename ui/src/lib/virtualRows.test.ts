import { describe, expect, it } from "vitest";
import { rowRange, rowTop, scrollTopForRow, visibleRows } from "./virtualRows";

const layout = { rowHeight: 100, rowCount: 1000, paddingTop: 20 };

describe("visibleRows", () => {
  it("covers the viewport plus overscan at the top", () => {
    expect(visibleRows(0, 350, layout, 2)).toEqual({ start: 0, end: 6 });
  });

  it("moves with the scroll position", () => {
    // Content offset 5000 - 20 = 4980 -> rows 49..(4980+350)/100=53.3 -> 54
    expect(visibleRows(5000, 350, layout, 2)).toEqual({ start: 47, end: 56 });
  });

  it("clamps at the end", () => {
    const end = visibleRows(1_000_000, 350, layout, 2);
    expect(end.end).toBe(1000);
    expect(end.start).toBeLessThan(1000);
  });

  it("handles empty lists and zero sizes", () => {
    expect(visibleRows(0, 350, { ...layout, rowCount: 0 })).toEqual({ start: 0, end: 0 });
    expect(visibleRows(0, 350, { ...layout, rowHeight: 0 })).toEqual({ start: 0, end: 0 });
    expect(visibleRows(0, 0, layout, 0)).toEqual({ start: 0, end: 0 });
  });
});

describe("scrollTopForRow", () => {
  it("keeps the position when the row is visible", () => {
    expect(scrollTopForRow(1, 0, 400, layout)).toBe(0);
  });

  it("scrolls down just enough to reveal a row below", () => {
    // Row 10: top 1020, bottom 1120 -> scrollTop 1120 - 400 + 20 = 740
    expect(scrollTopForRow(10, 0, 400, layout)).toBe(740);
  });

  it("scrolls up to reveal a row above", () => {
    expect(scrollTopForRow(2, 5000, 400, layout)).toBe(rowTop(2, layout) - 20);
    expect(scrollTopForRow(0, 5000, 400, layout)).toBe(0);
  });
});

describe("rowRange", () => {
  it("lists indices of a window", () => {
    expect(rowRange({ start: 3, end: 6 })).toEqual([3, 4, 5]);
    expect(rowRange({ start: 0, end: 0 })).toEqual([]);
  });
});
