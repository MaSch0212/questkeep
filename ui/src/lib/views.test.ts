import { describe, expect, it } from "vitest";
import type { GameQuery } from "../bindings/GameQuery";
import type { LibraryView } from "../bindings/LibraryView";
import type { ViewSection } from "../bindings/ViewSection";
import { makeGame } from "../test-utils";
import { defaultQuery } from "./query";
import {
  activeFilterCount,
  applyDrop,
  clearedFilters,
  differsFromSaved,
  dropTargetAt,
  effectiveState,
  matchesFilters,
  nudgeSection,
  nudgeView,
  sectionOrder,
  viewForGame,
  withChange,
  type EditRow,
} from "./views";

function view(id: string, query: Partial<GameQuery> = {}): LibraryView {
  return { id, name: id, icon: "bookmark", query: { ...defaultQuery(), ...query }, layout: "grid" };
}

function sections(): ViewSection[] {
  return [
    { id: "a", name: "A", views: [view("all"), view("fav", { favorite: "yes" })] },
    { id: "b", name: "B", views: [view("hidden", { hidden: "hidden" })] },
  ];
}

const order = (list: ViewSection[]) =>
  sectionOrder(list).map((s) => `${s.id}:${s.views.join(",")}`);

const q = (patch: Partial<GameQuery>) => ({ ...defaultQuery(), ...patch });

describe("drafts", () => {
  it("keeps a change as a draft and drops it once the view is back to its saved state", () => {
    const fav = view("fav", { favorite: "yes" });
    let drafts = withChange({}, fav, (s) => ({ ...s, layout: "list" }));
    expect(effectiveState(fav, drafts).layout).toBe("list");
    expect(Object.keys(drafts)).toEqual(["fav"]);
    drafts = withChange(drafts, fav, (s) => ({ ...s, layout: "grid" }));
    expect(drafts).toEqual({});
  });

  it("ignores the keyword and the order of stores when comparing", () => {
    const steam = view("s", { sources: ["steam", "gog"] });
    expect(
      differsFromSaved(steam, {
        query: { ...steam.query, search: "zelda", sources: ["gog", "steam"] },
        layout: "grid",
      }),
    ).toBe(false);
    expect(
      differsFromSaved(steam, { query: { ...steam.query, descending: true }, layout: "grid" }),
    ).toBe(true);
  });
});

describe("filters", () => {
  it("counts the filters, not sort or layout", () => {
    expect(activeFilterCount(defaultQuery())).toBe(0);
    const query: GameQuery = {
      ...defaultQuery(),
      sources: ["steam", "gog"],
      installed: "no",
      hidden: "all",
      sort: "playtime",
    };
    expect(activeFilterCount(query)).toBe(3);
    expect(activeFilterCount(clearedFilters(query))).toBe(0);
    expect(clearedFilters(query).sort).toBe("playtime");
  });

  it("matches games like the core does", () => {
    const played = makeGame({ playCount: 1 });
    expect(matchesFilters(q({ played: "played" }), played)).toBe(true);
    expect(matchesFilters(q({ played: "never" }), played)).toBe(false);
    expect(matchesFilters(q({}), makeGame({ hidden: true }))).toBe(false);
    expect(matchesFilters(q({ hidden: "all" }), makeGame({ hidden: true }))).toBe(true);
    expect(matchesFilters(q({ hidden: "hidden" }), makeGame())).toBe(false);
    expect(matchesFilters(q({ installed: "no" }), makeGame({ installed: false }))).toBe(true);
    expect(matchesFilters(q({ sources: ["steam"] }), makeGame({ source: "gog" }))).toBe(false);
    expect(matchesFilters(q({ favorite: "yes" }), makeGame({ favorite: true }))).toBe(true);
  });

  it("finds the view to show a game in", () => {
    const list = sections();
    const hidden = makeGame({ hidden: true });
    expect(viewForGame(list, {}, "fav", makeGame({ favorite: true }))?.id).toBe("fav");
    expect(viewForGame(list, {}, "fav", makeGame())?.id).toBe("all");
    expect(viewForGame(list, {}, "all", hidden)?.id).toBe("hidden");
    expect(viewForGame(list.slice(0, 1), {}, "all", hidden)).toBeNull();
  });
});

describe("arranging", () => {
  it("moves views within and between sections, counting from where they are", () => {
    const list = sections();
    expect(
      order(
        applyDrop(list, { kind: "view", id: "all" }, { kind: "view", sectionId: "a", index: 2 }),
      ),
    ).toEqual(["a:fav,all", "b:hidden"]);
    expect(
      order(
        applyDrop(list, { kind: "view", id: "fav" }, { kind: "view", sectionId: "b", index: 0 }),
      ),
    ).toEqual(["a:all", "b:fav,hidden"]);
    expect(
      order(applyDrop(list, { kind: "section", id: "b" }, { kind: "section", index: 0 })),
    ).toEqual(["b:hidden", "a:all,fav"]);
    expect(
      order(applyDrop(list, { kind: "section", id: "a" }, { kind: "section", index: 2 })),
    ).toEqual(["b:hidden", "a:all,fav"]);
  });

  it("nudges a view across a section edge and sections past each other", () => {
    const list = sections();
    expect(order(nudgeView(list, "fav", 1))).toEqual(["a:all", "b:fav,hidden"]);
    expect(order(nudgeView(list, "hidden", -1))).toEqual(["a:all,fav,hidden", "b:"]);
    expect(order(nudgeView(list, "all", -1))).toEqual(["a:all,fav", "b:hidden"]);
    expect(order(nudgeSection(list, "a", 1))).toEqual(["b:hidden", "a:all,fav"]);
  });

  it("targets the slot under the pointer", () => {
    // Rows: [section a] all fav [section b] hidden
    const rows: EditRow[] = [
      { kind: "section", sectionId: "a", index: 0, middle: 10 },
      { kind: "view", sectionId: "a", index: 0, middle: 30 },
      { kind: "view", sectionId: "a", index: 1, middle: 50 },
      { kind: "section", sectionId: "b", index: 1, middle: 70 },
      { kind: "view", sectionId: "b", index: 0, middle: 90 },
    ];
    const ids = ["a", "b"];
    const sizes = { a: 2, b: 1 };
    expect(dropTargetAt(rows, ids, sizes, "view", 35)).toEqual({
      kind: "view",
      sectionId: "a",
      index: 1,
    });
    expect(dropTargetAt(rows, ids, sizes, "view", 60)).toEqual({
      kind: "view",
      sectionId: "a",
      index: 2,
    });
    expect(dropTargetAt(rows, ids, sizes, "view", 5)).toEqual({
      kind: "view",
      sectionId: "a",
      index: 0,
    });
    expect(dropTargetAt(rows, ids, sizes, "view", 200)).toEqual({
      kind: "view",
      sectionId: "b",
      index: 1,
    });
    expect(dropTargetAt(rows, ids, sizes, "section", 60)).toEqual({ kind: "section", index: 1 });
    expect(dropTargetAt(rows, ids, sizes, "section", 200)).toEqual({ kind: "section", index: 2 });
  });
});
