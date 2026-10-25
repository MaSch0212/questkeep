import { describe, expect, it } from "vitest";
import type { Category } from "../bindings/Category";
import { makeGame } from "../test-utils";
import { nextIndex, type NavKey } from "./gridNav";
import {
  groupGames,
  layoutGroups,
  navigateSlots,
  scrollTopForRow,
  slotKey,
  visibleRowRange,
  type GameGroup,
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

const hades = makeGame({ id: "h", name: "Hades", source: "steam", categories: ["c1", "c2"] });
const celeste = makeGame({ id: "c", name: "Celeste", source: "epic", categories: ["c1"] });
const doom = makeGame({ id: "d", name: "Doom", source: "steam", categories: ["gone"] });
const games = [celeste, doom, hades];

const names = (groups: GameGroup[]) =>
  groups.map((g) => `${g.label}: ${g.games.map((game) => game.name).join(", ")}`);

describe("groupGames", () => {
  it("puts a game into each of its categories, in the categories' order, uncategorized last", () => {
    const groups = groupGames(games, "category", categories, sourceName);
    expect(names(groups)).toEqual(["co-op: Hades", "RPG: Celeste, Hades", "No category: Doom"]);
    expect(groups.at(-1)?.key).toBe(NO_CATEGORY);
    const reordered = groupGames(games, "category", categories.toReversed(), sourceName);
    expect(names(reordered)).toEqual(["RPG: Celeste, Hades", "co-op: Hades", "No category: Doom"]);
  });

  it("groups by store name and leaves ungrouped views as one group", () => {
    const bySource = groupGames(games, "source", categories, sourceName);
    expect(names(bySource)).toEqual(["Epic Games: Celeste", "Steam: Doom, Hades"]);
    expect(bySource[1]?.source).toBe("steam");
    const none = groupGames(games, "none", categories, sourceName);
    expect(none).toHaveLength(1);
    expect(none[0]?.key).toBe("");
  });
});

describe("layoutGroups", () => {
  const groups = groupGames(games, "category", categories, sourceName);
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
    const flat = layoutGroups(groupGames(games, "none", [], sourceName), {
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
  const groups = groupGames(games, "category", categories, sourceName);
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
    const flat = layoutGroups(groupGames(many, "none", [], sourceName), {
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
