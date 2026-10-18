import { describe, expect, it } from "vitest";
import { isNavKey, nextIndex } from "./gridNav";

// 10 items, 4 columns:  0 1 2 3 / 4 5 6 7 / 8 9
describe("nextIndex", () => {
  it("returns -1 for an empty list", () => {
    expect(nextIndex(-1, "ArrowDown", 4, 0, 3)).toBe(-1);
  });
  it("starts at the first tile when nothing is selected", () => {
    expect(nextIndex(-1, "ArrowRight", 4, 10, 3)).toBe(0);
    expect(nextIndex(-1, "ArrowDown", 4, 10, 3)).toBe(0);
    expect(nextIndex(-1, "End", 4, 10, 3)).toBe(9);
  });
  it("wraps left/right across rows and clamps at the ends", () => {
    expect(nextIndex(3, "ArrowRight", 4, 10, 3)).toBe(4);
    expect(nextIndex(4, "ArrowLeft", 4, 10, 3)).toBe(3);
    expect(nextIndex(0, "ArrowLeft", 4, 10, 3)).toBe(0);
    expect(nextIndex(9, "ArrowRight", 4, 10, 3)).toBe(9);
  });
  it("moves up and down by one row", () => {
    expect(nextIndex(5, "ArrowDown", 4, 10, 3)).toBe(9);
    expect(nextIndex(5, "ArrowUp", 4, 10, 3)).toBe(1);
    expect(nextIndex(1, "ArrowUp", 4, 10, 3)).toBe(1);
  });
  it("goes to the last tile when the row below is short", () => {
    expect(nextIndex(7, "ArrowDown", 4, 10, 3)).toBe(9);
    expect(nextIndex(9, "ArrowDown", 4, 10, 3)).toBe(9);
    expect(nextIndex(8, "ArrowDown", 4, 10, 3)).toBe(8);
  });
  it("handles Home and End", () => {
    expect(nextIndex(6, "Home", 4, 10, 3)).toBe(0);
    expect(nextIndex(6, "End", 4, 10, 3)).toBe(9);
  });
  it("pages by pageRows rows and clamps", () => {
    expect(nextIndex(0, "PageDown", 4, 100, 3)).toBe(12);
    expect(nextIndex(50, "PageUp", 4, 100, 3)).toBe(38);
    expect(nextIndex(2, "PageUp", 4, 100, 3)).toBe(0);
    expect(nextIndex(95, "PageDown", 4, 100, 3)).toBe(99);
  });
  it("sanitises degenerate columns and pageRows", () => {
    expect(nextIndex(1, "ArrowDown", 0, 5, 0)).toBe(2);
    expect(nextIndex(0, "PageDown", 1, 5, 0)).toBe(1);
  });
  it("resets out-of-range selections", () => {
    expect(nextIndex(50, "ArrowLeft", 4, 10, 3)).toBe(0);
  });
  it("recognises nav keys", () => {
    expect(isNavKey("PageUp")).toBe(true);
    expect(isNavKey("a")).toBe(false);
  });
});
