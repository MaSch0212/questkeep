import { describe, expect, it } from "vitest";
import type { Category } from "../bindings/Category";
import { intlLocale } from "../i18n";
import { makeGame } from "../test-utils";
import { nextIndex, type NavKey } from "./gridNav";
import {
  groupGames,
  groupStart,
  groupSummary,
  layoutGroups,
  navigateSlots,
  scrollTopForRow,
  slotKey,
  stickyHeader,
  visibleRowRange,
  type GameGroup,
  type GroupContext,
} from "./grouping";
import { NO_CATEGORY } from "./views";

const category = (id: string, name: string): Category => ({
  id,
  name,
  gameCount: 0,
  sources: [],
});
const categories = [category("c2", "co-op"), category("c1", "RPG"), category("c3", "Empty")];
const sourceName = (id: string) => ({ steam: "Steam", epic: "Epic Games" })[id] ?? id;
const ctx = (list: Category[]): GroupContext => ({ categories: list, sourceName, statuses: [] });

const hades = makeGame({ id: "h", name: "Hades", source: "steam", categories: ["c1", "c2"] });
const celeste = makeGame({ id: "c", name: "Celeste", source: "epic", categories: ["c1"] });
const doom = makeGame({ id: "d", name: "Doom", source: "steam", categories: ["gone"] });
const games = [celeste, doom, hades];

const names = (groups: GameGroup[]) =>
  groups.map((g) => `${g.label}: ${g.games.map((game) => game.name).join(", ")}`);

describe("groupGames", () => {
  it("puts a game into each of its categories, in the categories' order, uncategorized last", () => {
    const groups = groupGames(games, "category", ctx(categories));
    expect(names(groups)).toEqual(["co-op: Hades", "RPG: Celeste, Hades", "No category: Doom"]);
    expect(groups.at(-1)?.key).toBe(NO_CATEGORY);
    const reordered = groupGames(games, "category", ctx(categories.toReversed()));
    expect(names(reordered)).toEqual(["RPG: Celeste, Hades", "co-op: Hades", "No category: Doom"]);
  });

  it("groups by list values once per spelling, by name, games without one last", () => {
    const a = makeGame({ id: "a", name: "A", tags: ["Co-op", "Roguelike"] });
    const b = makeGame({ id: "b", name: "B", tags: ["co-op", "CO OP"] });
    const c = makeGame({ id: "c", name: "C" });
    expect(names(groupGames([a, b, c], "tag", ctx([])))).toEqual([
      "Co-op: A, B",
      "Roguelike: A",
      "None: C",
    ]);
  });

  it("orders statuses by the list, years newest first, buckets in their order", () => {
    const statuses = [
      { id: "s1", name: "Not Played", gameCount: 0 },
      { id: "s2", name: "Beaten", gameCount: 0 },
    ];
    const a = makeGame({ id: "a", name: "A", completionStatus: "s2", releaseDate: "2019-05-01" });
    const b = makeGame({ id: "b", name: "B", completionStatus: "s1", releaseDate: "2021" });
    const c = makeGame({ id: "c", name: "C", completionStatus: null, playtimeSeconds: 40000 });
    const byStatus = groupGames([a, b, c], "completionStatus", { ...ctx([]), statuses });
    expect(names(byStatus)).toEqual(["Not Played: B", "Beaten: A", "No status: C"]);
    expect(names(groupGames([a, b, c], "releaseYear", ctx([])))).toEqual([
      "2021: B",
      "2019: A",
      "None: C",
    ]);
    expect(names(groupGames([a, b, c], "playtime", ctx([])))).toEqual([
      "Never played: A, B",
      "10–50 hours: C",
    ]);
    const now = Date.parse("2026-10-31T12:00:00Z");
    const recent = makeGame({ id: "r", name: "R", lastPlayed: "2026-10-31T08:00:00Z" });
    const old = makeGame({ id: "o", name: "O", lastPlayed: "2024-01-01T00:00:00Z" });
    expect(names(groupGames([old, recent, c], "lastPlayed", { ...ctx([]), now }))).toEqual([
      "Today: R",
      "Over a year ago: O",
      "Never played: C",
    ]);
  });

  it("groups by store name and leaves ungrouped views as one group", () => {
    const bySource = groupGames(games, "source", ctx(categories));
    expect(names(bySource)).toEqual(["Epic Games: Celeste", "Steam: Doom, Hades"]);
    expect(bySource[1]?.source).toBe("steam");
    const ordered = groupGames(games, "source", { ...ctx(categories), storeOrder: ["steam"] });
    expect(names(ordered)).toEqual(["Steam: Doom, Hades", "Epic Games: Celeste"]);
    const none = groupGames(games, "none", ctx(categories));
    expect(none).toHaveLength(1);
    expect(none[0]?.key).toBe("");
  });
});

describe("layoutGroups", () => {
  const groups = groupGames(games, "category", ctx(categories));
  const options = {
    columns: 2,
    rowHeight: 100,
    headerHeight: 30,
    padding: 10,
    headers: true,
    collapsed: new Set<string>(),
  };

  it("adds header rows, splits games into rows and offsets them", () => {
    const layout = layoutGroups(groups, options);
    expect(layout.rows.map((r) => [r.kind, r.top])).toEqual([
      ["header", 10],
      ["games", 40],
      ["header", 140],
      ["games", 170],
      ["header", 270],
      ["games", 300],
    ]);
    expect(layout.height).toBe(410);
    expect(layout.slots.map((s) => [s.game.name, s.row, s.column])).toEqual([
      ["Hades", 1, 0],
      ["Celeste", 3, 0],
      ["Hades", 3, 1],
      ["Doom", 5, 0],
    ]);
    expect(layout.slots.map(slotKey)).toEqual(["h:c2", "c:c1", "h:c1", `d:${NO_CATEGORY}`]);
  });

  it("collapsed groups keep only their header; ungrouped views have no headers", () => {
    const collapsed = layoutGroups(groups, { ...options, collapsed: new Set(["c1"]) });
    expect(collapsed.rows.map((r) => r.kind)).toEqual([
      "header",
      "games",
      "header",
      "header",
      "games",
    ]);
    expect(collapsed.slots.map((s) => s.game.name)).toEqual(["Hades", "Doom"]);
    const flat = layoutGroups(groupGames(games, "none", ctx([])), {
      ...options,
      headers: false,
    });
    expect(flat.rows.every((r) => r.kind === "games")).toBe(true);
    expect(flat.slots.map(slotKey)).toEqual(["c", "d", "h"]);
    expect(layoutGroups([], options).height).toBe(0);
  });

  it("finds the visible rows and scrolls rows into view with their header", () => {
    const layout = layoutGroups(groups, options);
    expect(visibleRowRange(layout, 0, 100, 0)).toEqual({ start: 0, end: 2 });
    expect(visibleRowRange(layout, 150, 100, 0)).toEqual({ start: 2, end: 4 });
    expect(visibleRowRange(layout, 150, 100, 1)).toEqual({ start: 1, end: 5 });
    expect(scrollTopForRow(layout, 3, 0, 100, 10)).toBe(180);
    expect(scrollTopForRow(layout, 3, 400, 100, 10)).toBe(140);
    expect(scrollTopForRow(layout, 1, 50, 100, 10)).toBe(0);
    expect(scrollTopForRow(layout, 3, 150, 200, 10)).toBe(140);
    expect(scrollTopForRow(layout, 3, 140, 200, 10)).toBe(140);
  });
});

describe("navigateSlots", () => {
  const groups = groupGames(games, "category", ctx(categories));
  const layout = layoutGroups(groups, {
    columns: 2,
    rowHeight: 100,
    headerHeight: 30,
    padding: 0,
    headers: true,
    collapsed: new Set<string>(),
  });

  it("moves across groups by row and column", () => {
    expect(navigateSlots(layout, -1, "ArrowDown", 3)).toBe(0);
    expect(navigateSlots(layout, 0, "ArrowDown", 3)).toBe(1);
    expect(navigateSlots(layout, 2, "ArrowDown", 3)).toBe(3);
    expect(navigateSlots(layout, 2, "ArrowUp", 3)).toBe(0);
    expect(navigateSlots(layout, 0, "ArrowUp", 3)).toBe(0);
    expect(navigateSlots(layout, 3, "ArrowDown", 3)).toBe(3);
    expect(navigateSlots(layout, 0, "ArrowRight", 3)).toBe(1);
    expect(navigateSlots(layout, 3, "ArrowRight", 3)).toBe(3);
    expect(navigateSlots(layout, 0, "PageDown", 5)).toBe(3);
    expect(navigateSlots(layout, 3, "PageDown", 5)).toBe(3);
    expect(navigateSlots(layout, 3, "PageUp", 1)).toBe(1);
    expect(navigateSlots(layout, 2, "End", 1)).toBe(3);
    expect(navigateSlots(layoutGroups([], { ...layoutOptions() }), 0, "Home", 1)).toBe(-1);
  });

  it("matches the plain grid navigation when ungrouped", () => {
    const many = Array.from({ length: 11 }, (_, i) => makeGame({ id: `g${i}`, name: `G${i}` }));
    const flat = layoutGroups(groupGames(many, "none", ctx([])), {
      ...layoutOptions(),
      columns: 4,
      headers: false,
    });
    const keys: NavKey[] = ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End"];
    for (let current = 0; current < many.length; current++) {
      for (const key of keys) {
        expect([current, key, navigateSlots(flat, current, key, 2)]).toEqual([
          current,
          key,
          nextIndex(current, key, 4, many.length, 2),
        ]);
      }
    }
  });
});

function layoutOptions() {
  return {
    columns: 2,
    rowHeight: 100,
    headerHeight: 30,
    padding: 0,
    headers: true,
    collapsed: new Set<string>(),
  };
}

describe("stickyHeader", () => {
  // Two groups, one row each: header 0-40 (after 10 padding), row 50-150, header 150-190, row 190-290.
  const grouped = layoutGroups(
    [
      { key: "a", label: "A", source: null, games: [hades] },
      { key: "b", label: "B", source: null, games: [celeste] },
    ],
    {
      columns: 4,
      rowHeight: 100,
      headerHeight: 40,
      padding: 10,
      headers: true,
      collapsed: new Set(),
    },
  );
  const pinned = (scrollTop: number) => {
    const at = stickyHeader(grouped, scrollTop);
    return at && [at.row.group.key, at.offset];
  };

  it("pins the header scrolled past and lets the next one push it up", () => {
    expect(pinned(0)).toBeNull();
    expect(pinned(10)).toEqual(["a", 0]);
    expect(pinned(100)).toEqual(["a", 0]);
    expect(pinned(130)).toEqual(["a", -20]);
    expect(pinned(150)).toEqual(["b", 0]);
    expect(pinned(1000)).toEqual(["b", 0]);
  });

  it("pins nothing in an ungrouped view", () => {
    const flat = layoutGroups(groupGames(games, "none", ctx(categories)), {
      columns: 4,
      rowHeight: 100,
      headerHeight: 40,
      padding: 10,
      headers: false,
      collapsed: new Set(),
    });
    expect(stickyHeader(flat, 500)).toBeNull();
  });
});

describe("groupSummary", () => {
  it("counts installed games and adds up playtime in hours", () => {
    expect(
      groupSummary([
        makeGame({ id: "1", installed: true, playtimeSeconds: 3600 * 1500 }),
        makeGame({ id: "2", installed: true, playtimeSeconds: 3600 * 810 + 1700 }),
        makeGame({ id: "3", installed: false, playtimeSeconds: 0 }),
      ]),
    ).toBe(`2 installed · ${(2310).toLocaleString(intlLocale())}h`);
    expect(groupSummary([makeGame({ id: "1", installed: true, playtimeSeconds: 0 })])).toBe(
      "1 installed",
    );
    expect(groupSummary([makeGame({ id: "1", installed: false, playtimeSeconds: 7200 })])).toBe(
      "None installed · 2h",
    );
  });
});

describe("groupStart", () => {
  it("finds a group's header offset and first game", () => {
    const a = makeGame({ id: "a", name: "A", source: "steam" });
    const b = makeGame({ id: "b", name: "B", source: "gog" });
    const groups: GameGroup[] = [
      { key: "gog", label: "GOG", source: "gog", games: [b] },
      { key: "steam", label: "Steam", source: "steam", games: [a] },
    ];
    const options = {
      columns: 1,
      rowHeight: 30,
      headerHeight: 40,
      padding: 8,
      headers: true,
      collapsed: new Set<string>(),
    };
    const layout = layoutGroups(groups, options);
    expect(groupStart(layout, "steam")).toEqual({ top: 8 + 40 + 30, slot: 1 });
    expect(groupStart(layout, "missing")).toBeNull();
    const folded = layoutGroups(groups, { ...options, collapsed: new Set(["steam"]) });
    expect(groupStart(folded, "steam")).toEqual({ top: 8 + 40 + 30, slot: -1 });
  });
});
