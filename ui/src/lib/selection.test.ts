import { describe, expect, it } from "vitest";
import { makeGame } from "../test-utils";
import {
  categoryCoverage,
  common,
  coverFan,
  rangeSelection,
  storeCounts,
  toggleSelected,
} from "./selection";

describe("toggleSelected", () => {
  it("starts from the single selection and keeps selection order", () => {
    expect([...toggleSelected(new Set(), "a", "b")]).toEqual(["a", "b"]);
    expect([...toggleSelected(new Set(), null, "b")]).toEqual(["b"]);
    expect([...toggleSelected(new Set(["a", "b"]), "b", "a")]).toEqual(["b"]);
    expect([...toggleSelected(new Set(["b"]), "b", "c")]).toEqual(["b", "c"]);
  });
});

describe("rangeSelection", () => {
  const order = ["a", "b", "c", "b", "d"];

  it("selects from the anchor to the target in either direction, each game once", () => {
    expect(rangeSelection(order, "a", "c")).toEqual(["a", "b", "c"]);
    expect(rangeSelection(order, "d", "b")).toEqual(["b", "c", "d"]);
    expect(rangeSelection(order, "c", "c")).toEqual(["c"]);
  });

  it("selects only the target without a usable anchor", () => {
    expect(rangeSelection(order, null, "c")).toEqual(["c"]);
    expect(rangeSelection(order, "gone", "c")).toEqual(["c"]);
  });
});

describe("coverFan", () => {
  it("fans up to eight covers to fit the width", () => {
    expect(coverFan(3, 400, 100, 20)).toEqual({ shown: 3, step: 60 });
    expect(coverFan(20, 400, 100, 20)).toEqual({ shown: 8, step: 300 / 7 });
    expect(coverFan(20, 160, 100, 20)).toEqual({ shown: 4, step: 20 });
    expect(coverFan(1, 400, 100, 20)).toEqual({ shown: 1, step: 0 });
    expect(coverFan(0, 400, 100, 20)).toEqual({ shown: 0, step: 0 });
  });
});

describe("shared values", () => {
  const a = makeGame({ id: "a", favorite: true, categories: ["rpg", "coop"] });
  const b = makeGame({ id: "b", favorite: true, categories: ["rpg"] });
  const c = makeGame({ id: "c", favorite: false, categories: [] });

  it("tells a common value from a mixed one", () => {
    expect(common([a, b], (g) => g.favorite)).toBe(true);
    expect(common([a, c], (g) => g.favorite)).toBe("mixed");
    expect(common([], (g) => g.favorite)).toBeUndefined();
  });

  it("tells whether all, some or none have a category", () => {
    expect(categoryCoverage([a, b], "rpg")).toBe("all");
    expect(categoryCoverage([a, b], "coop")).toBe("some");
    expect(categoryCoverage([a, b, c], "x")).toBe("none");
  });

  it("counts games per store, most first", () => {
    const steam = makeGame({ id: "s1", source: "steam" });
    const steam2 = makeGame({ id: "s2", source: "steam" });
    const gog = makeGame({ id: "g", source: "gog" });
    expect(storeCounts([gog, steam, steam2], (s) => s.toUpperCase())).toBe("STEAM 2 · GOG 1");
  });
});
