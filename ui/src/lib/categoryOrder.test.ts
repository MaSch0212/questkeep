import { describe, expect, it } from "vitest";
import { dropIndexAt, moveBefore, nudge } from "./categoryOrder";

const ids = ["a", "b", "c", "d"];

describe("category order", () => {
  it("moves an item before another", () => {
    expect(moveBefore(ids, "a", 3)).toEqual(["b", "c", "a", "d"]);
    expect(moveBefore(ids, "d", 0)).toEqual(["d", "a", "b", "c"]);
    expect(moveBefore(ids, "b", 4)).toEqual(["a", "c", "d", "b"]);
    expect(moveBefore(ids, "b", 1)).toEqual(ids);
    expect(moveBefore(ids, "x", 0)).toEqual(ids);
  });

  it("nudges one step and stops at the ends", () => {
    expect(nudge(ids, "b", -1)).toEqual(["b", "a", "c", "d"]);
    expect(nudge(ids, "b", 1)).toEqual(["a", "c", "b", "d"]);
    expect(nudge(ids, "a", -1)).toEqual(ids);
    expect(nudge(ids, "d", 1)).toEqual(ids);
  });

  it("finds the drop index from row middles", () => {
    const middles = [10, 30, 50];
    expect(dropIndexAt(middles, 0)).toBe(0);
    expect(dropIndexAt(middles, 20)).toBe(1);
    expect(dropIndexAt(middles, 60)).toBe(3);
  });
});
