import { describe, expect, it } from "vitest";
import type { FilterGroup } from "../bindings/FilterGroup";
import type { FilterNode } from "../bindings/FilterNode";
import type { TextFilter } from "../bindings/TextFilter";
import { makeGame } from "../test-utils";
import {
  activeTreeCount,
  pickedNames,
  appendTo,
  describeRange,
  duplicateAt,
  fromInput,
  isActive,
  matchesTree,
  moveBy,
  moveChoices,
  moveNode,
  NO_VALUE,
  pruneTree,
  rangeInfo,
  removeAt,
  sameTree,
  setMode,
  textPattern,
  toggleNegate,
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
        negated: false,
        parts: [
          { label: "Tags", value: "Co-op" },
          { label: "Genre", value: "Roguelike" },
        ],
      },
      {
        index: 1,
        joiner: "and",
        grouped: false,
        negated: false,
        parts: [{ label: "Playtime", value: "2 h +" }],
      },
    ]);
  });
});

describe("pickedNames", () => {
  it("shows the first names and counts the rest", () => {
    expect(pickedNames([])).toEqual({ shown: "", more: 0 });
    expect(pickedNames(["Steam", "GOG"])).toEqual({ shown: "Steam, GOG", more: 0 });
    expect(pickedNames(["Steam", "GOG", "Xbox", "EA app"])).toEqual({
      shown: "Steam, GOG",
      more: 2,
    });
  });
});

const byName = (
  op: TextFilter["op"],
  value: string,
  extra: Partial<TextFilter> = {},
): FilterNode => ({
  kind: "text",
  field: "name",
  op,
  value,
  ...extra,
});
const allOf = (...items: FilterNode[]): FilterGroup => ({ mode: "all", items });
const text = (field: TextFilter["field"], op: TextFilter["op"], value = ""): FilterGroup =>
  allOf({ kind: "text", field, op, value });

describe("text filters and negation", () => {
  const trine = makeGame({ name: "Trine 4", notes: null });
  const brutal = makeGame({ name: "Brütal Legend" });

  it("compares without case unless asked, with every operation", () => {
    expect(matchesTree(allOf(byName("startsWith", "TRINE")), trine)).toBe(true);
    expect(matchesTree(allOf(byName("startsWith", "TRINE", { caseSensitive: true })), trine)).toBe(
      false,
    );
    expect(matchesTree(allOf(byName("contains", "BRÜT")), brutal)).toBe(true);
    expect(matchesTree(allOf(byName("endsWith", " 4")), trine)).toBe(true);
    expect(matchesTree(allOf(byName("equals", "trine")), trine)).toBe(false);
    expect(matchesTree(allOf(byName("matches", String.raw`^trine \d$`)), trine)).toBe(true);
    // An invalid pattern matches nothing.
    expect(matchesTree(allOf(byName("matches", "trine (")), trine)).toBe(false);
    expect(textPattern({ kind: "text", field: "name", op: "matches", value: "(" } as never)).toBe(
      null,
    );
  });

  it("negates filters and groups, games without a value included", () => {
    const notes: FilterNode = { kind: "text", field: "notes", op: "contains", value: "x" };
    expect(matchesTree(allOf(notes), trine)).toBe(false);
    expect(matchesTree(allOf({ ...notes, negate: true }), trine)).toBe(true);
    const notEither: FilterNode = {
      kind: "group",
      mode: "any",
      items: [byName("startsWith", "trine"), tags("Co-op")],
      negate: true,
    };
    expect(matchesTree(allOf(notEither), trine)).toBe(false);
    expect(matchesTree(allOf(notEither), brutal)).toBe(true);
    expect(matchesTree(allOf({ ...playtime(1, null), negate: true }), brutal)).toBe(true);
  });

  it("matches several texts, value names and the empty checks", () => {
    const game = makeGame({
      name: "Alpha",
      notes: "  ",
      description: "A legendary quest",
      developers: ["Ubisoft Montreal"],
      sourceGameId: "440",
      playAction: {
        kind: "file",
        target: "C:\\Games\\alpha.exe",
        args: "-windowed",
        workingDir: null,
        tracking: "default",
        trackingPath: null,
      },
    });
    expect(matchesTree(text("anyText", "contains", "legend"), game)).toBe(true);
    expect(matchesTree(text("developer", "startsWith", "ubisoft"), game)).toBe(true);
    expect(matchesTree(text("publisher", "isEmpty"), game)).toBe(true);
    expect(matchesTree(text("notes", "isEmpty"), game)).toBe(true);
    expect(matchesTree(text("notes", "hasValue"), game)).toBe(false);
    expect(matchesTree(text("sourceGameId", "equals", "440"), game)).toBe(true);
    expect(matchesTree(text("launchTarget", "endsWith", "ALPHA.EXE"), game)).toBe(true);
    expect(matchesTree(text("launchArgs", "contains", "window"), game)).toBe(true);
    // The empty checks are active without text.
    expect(isActive({ kind: "text", field: "notes", op: "isEmpty", value: "" })).toBe(true);
    expect(isActive({ kind: "text", field: "notes", op: "contains", value: "" })).toBe(false);
  });

  it("keeps negation when pruning, comparing and describing", () => {
    const one: FilterNode = { kind: "group", mode: "all", items: [tags("Co-op")], negate: true };
    expect(pruneTree(allOf(one)).items).toEqual([{ ...tags("Co-op"), negate: true }]);
    expect(sameTree(allOf(tags("a")), allOf({ ...tags("a"), negate: true }))).toBe(false);
    expect(sameTree(allOf(byName("contains", "a")), allOf(byName("contains", "a")))).toBe(true);
    expect(sameTree(allOf(byName("contains", "a")), allOf(byName("endsWith", "a")))).toBe(false);
    const toggled = toggleNegate(allOf(tags("a")), [0]);
    expect(toggled.items[0]?.negate).toBe(true);
    expect(toggleNegate(toggled, [0]).items[0]?.negate).toBe(false);
    const [pill] = treePills(allOf({ ...byName("contains", "trine"), negate: true }), () => "");
    expect(pill?.parts[0]).toEqual({ label: "Name", value: "not contains “trine”" });
    const [groupPill] = treePills(
      allOf({ ...one, items: [tags("a"), tags("b")] }),
      (_, key) => key,
    );
    expect(groupPill?.negated).toBe(true);
  });
});

const sub = (...items: FilterNode[]): FilterNode => ({ kind: "group", mode: "any", items });
/** A tree as nested arrays of its tag values, to compare moves at a glance. */
const shape = (at: FilterGroup): unknown =>
  at.items.map((node) =>
    node.kind === "group" ? shape(node) : node.kind === "list" ? node.values[0] : "?",
  );

describe("moving filters", () => {
  const a = tags("a");
  const b = tags("b");
  const c = tags("c");

  it("reorders within a group", () => {
    const flat = allOf(a, b, c);
    expect(shape(moveNode(flat, [0], [2], "after"))).toEqual(["b", "c", "a"]);
    expect(shape(moveNode(flat, [2], [0], "before"))).toEqual(["c", "a", "b"]);
    expect(shape(moveNode(flat, [0], [1], "before"))).toEqual(["a", "b", "c"]);
  });

  it("moves into, out of and between groups", () => {
    const mixed = allOf(a, sub(b), c);
    expect(shape(moveNode(mixed, [0], [1], "into"))).toEqual([["b", "a"], "c"]);
    expect(shape(moveNode(mixed, [2], [1, 0], "before"))).toEqual(["a", ["c", "b"]]);
    expect(shape(moveNode(mixed, [1, 0], [0], "before"))).toEqual(["b", "a", [], "c"]);
    // Into a group that comes after the moved node.
    const nested = allOf(a, sub(b, sub(c)));
    expect(shape(moveNode(nested, [0], [1, 1], "into"))).toEqual([["b", ["c", "a"]]]);
  });

  it("never moves a group into itself", () => {
    const deep = allOf(sub(a, sub(b)));
    expect(moveNode(deep, [0], [0, 1], "into")).toBe(deep);
    expect(moveNode(deep, [0], [0], "into")).toBe(deep);
  });

  it("offers the keyboard moves that make sense", () => {
    const start = allOf(sub(a), b, c);
    expect(moveChoices(start, [1])).toEqual({ up: true, down: true, out: false, intoAbove: true });
    expect(moveChoices(start, [0, 0])).toEqual({
      up: false,
      down: false,
      out: true,
      intoAbove: false,
    });
    expect(shape(moveBy(start, [1], "intoAbove"))).toEqual([["a", "b"], "c"]);
    expect(shape(moveBy(start, [0, 0], "out"))).toEqual([[], "a", "b", "c"]);
    expect(shape(moveBy(start, [2], "up"))).toEqual([["a"], "c", "b"]);
  });
});
