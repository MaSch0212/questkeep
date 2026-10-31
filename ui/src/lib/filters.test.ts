import { describe, expect, it } from "vitest";
import type { FilterGroup } from "../bindings/FilterGroup";
import type { FilterNode } from "../bindings/FilterNode";
import { makeGame } from "../test-utils";
import {
  activeTreeCount,
  appendTo,
  describeRange,
  duplicateAt,
  fromInput,
  matchesTree,
  NO_VALUE,
  pruneTree,
  rangeInfo,
  removeAt,
  sameTree,
  setMode,
  toInput,
  treePills,
  ungroupAt,
  updateAt,
  valueKey,
} from "./filters";

const tags = (...values: string[]) =>
  ({ kind: "list", field: "tag", mode: "any", values }) as const;
const genre = (...values: string[]) =>
  ({ kind: "list", field: "genre", mode: "any", values }) as const;
const playtime = (min: number | null, max: number | null) =>
  ({ kind: "range", field: "playtime", min, max }) as const;

const group = (mode: FilterGroup["mode"], ...items: FilterNode[]): FilterGroup => ({ mode, items });

// Installed and (Tags: Co-op or Genre: Roguelike) and playtime >= 2 h.
const tree: FilterGroup = {
  mode: "all",
  items: [
    { kind: "group", mode: "any", items: [tags("coop"), genre("roguelike")] },
    playtime(7200, null),
  ],
};

describe("valueKey", () => {
  it("matches the core's normalisation", () => {
    expect(valueKey("Co-Op")).toBe("coop");
    expect(valueKey("C++ Café")).toBe("c++café");
  });
});

describe("matchesTree", () => {
  it("combines groups with all/any", () => {
    const coop = makeGame({ tags: ["Co-op"], playtimeSeconds: 9000 });
    const rogue = makeGame({ genres: ["Roguelike"], playtimeSeconds: 9000 });
    const short = makeGame({ tags: ["Co-op"], playtimeSeconds: 60 });
    const neither = makeGame({ tags: ["Puzzle"], playtimeSeconds: 9000 });
    expect([coop, rogue, short, neither].map((g) => matchesTree(tree, g))).toEqual([
      true,
      true,
      false,
      false,
    ]);
  });

  it("handles all/none modes, the no-value entry and empty rows", () => {
    const both = makeGame({ tags: ["Co-op", "Indie"] });
    const none = makeGame({ tags: [] });
    const all = group("all", { ...tags("coop", "indie"), mode: "all" });
    expect(matchesTree(all, both)).toBe(true);
    expect(matchesTree(all, makeGame({ tags: ["Co-op"] }))).toBe(false);
    const not = group("all", { ...tags("coop"), mode: "none" });
    expect(matchesTree(not, both)).toBe(false);
    expect(matchesTree(not, none)).toBe(true);
    const missing = group("all", tags(NO_VALUE));
    expect(matchesTree(missing, none)).toBe(true);
    expect(matchesTree(missing, both)).toBe(false);
    const empty = group("any", tags(), playtime(null, null));
    expect(matchesTree(empty, both)).toBe(true);
  });

  it("never matches a range on a game without a value", () => {
    const filter = group("all", { kind: "range", field: "criticScore", min: null, max: 100 });
    expect(matchesTree(filter, makeGame({ criticScore: null }))).toBe(false);
    expect(matchesTree(filter, makeGame({ criticScore: { value: 80, source: "x" } }))).toBe(true);
  });
});

describe("editing the tree", () => {
  it("updates, removes, appends and switches modes by path", () => {
    let next = updateAt(tree, [0, 1], () => genre("strategy"));
    expect(next.items[0]).toMatchObject({ items: [tags("coop"), genre("strategy")] });
    next = removeAt(next, [0, 0]);
    expect(next.items[0]).toMatchObject({ items: [genre("strategy")] });
    next = appendTo(next, [0], playtime(0, 0));
    expect(next.items[0]).toMatchObject({ items: [genre("strategy"), playtime(0, 0)] });
    next = setMode(setMode(next, [], "any"), [0], "all");
    expect(next.mode).toBe("any");
    expect(next.items[0]).toMatchObject({ mode: "all" });
    expect(tree.items[0]).toMatchObject({ mode: "any", items: [tags("coop"), genre("roguelike")] });
  });

  it("ungroups in place and duplicates after the node", () => {
    expect(ungroupAt(tree, [0]).items).toEqual([
      tags("coop"),
      genre("roguelike"),
      playtime(7200, null),
    ]);
    const copied = duplicateAt(tree, [1]);
    expect(copied.items).toHaveLength(3);
    expect(copied.items[2]).toEqual(playtime(7200, null));
  });

  it("counts and compares only filters that narrow", () => {
    expect(activeTreeCount(tree)).toBe(3);
    const withEmpty = appendTo(tree, [], tags());
    expect(activeTreeCount(withEmpty)).toBe(3);
    expect(sameTree(withEmpty, tree)).toBe(false);
    expect(sameTree(structuredClone(tree), tree)).toBe(true);
    expect(
      pruneTree({ mode: "all", items: [{ kind: "group", mode: "any", items: [tags("a")] }] }),
    ).toEqual({ mode: "all", items: [tags("a")] });
  });
});

describe("ranges", () => {
  it("converts hours and sizes both ways", () => {
    expect(fromInput("hours", "2,5")).toBe(9000);
    expect(toInput("hours", 9000)).toBe("2.5");
    expect(fromInput("gigabytes", "1")).toBe(1024 ** 3);
    expect(fromInput("number", "-3")).toBeNull();
    expect(fromInput("date", "2024-01-02")).toBe(Date.parse("2024-01-02") / 1000);
  });

  it("describes ranges", () => {
    const hours = rangeInfo("playtime");
    expect(describeRange(hours, 7200, 180000)).toBe("2 – 50 h");
    expect(describeRange(hours, 36000, null)).toBe("10 h +");
    expect(describeRange(hours, 0, 0)).toBe("0 h");
    expect(describeRange(rangeInfo("criticScore"), null, 50)).toBe("up to 50");
  });
});

const name = (_: string, key: string) => ({ coop: "Co-op", roguelike: "Roguelike" })[key] ?? key;

describe("treePills", () => {
  it("makes one pill per root filter and per group", () => {
    expect(treePills(tree, name)).toEqual([
      {
        index: 0,
        joiner: "or",
        grouped: true,
        parts: [
          { label: "Tags", value: "Co-op" },
          { label: "Genre", value: "Roguelike" },
        ],
      },
      { index: 1, joiner: "and", grouped: false, parts: [{ label: "Playtime", value: "2 h +" }] },
    ]);
  });
});
