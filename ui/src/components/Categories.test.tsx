import { fireEvent, render, screen, within } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Category } from "../bindings/Category";
import type { ViewSection } from "../bindings/ViewSection";
import * as api from "../lib/api";
import { defaultQuery } from "../lib/query";
import { store } from "../lib/store";
import { makeGame } from "../test-utils";
import { CategoryEditor } from "./CategoryEditor";
import { GameGrid } from "./GameGrid";
import { GameList } from "./GameList";
import { CategoriesSection } from "./settings/CategoriesSection";
import { TopBar } from "./TopBar";

vi.mock("../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));

const category = (
  id: string,
  name: string,
  gameCount = 1,
  sources: Category["sources"] = [],
): Category => ({ id, name, gameCount, sources });
const rpg = category("c1", "RPG", 2);
const coop = category("c2", "Co-op");

const hades = makeGame({ id: "h", name: "Hades", sortingName: "hades", categories: ["c1", "c2"] });
const celeste = makeGame({ id: "c", name: "Celeste", sortingName: "celeste", categories: ["c1"] });
const doom = makeGame({ id: "d", name: "Doom", sortingName: "doom" });

function sections(group: "none" | "category" = "none"): ViewSection[] {
  return [
    {
      id: "library",
      name: "Library",
      views: [
        {
          id: "all",
          name: "All games",
          icon: "library",
          query: defaultQuery(),
          layout: "grid",
          group,
        },
      ],
    },
  ];
}

async function setUp(group: "none" | "category" = "none") {
  vi.clearAllMocks();
  store.reset();
  vi.mocked(api.listViews).mockResolvedValue(sections(group));
  vi.mocked(api.listGames).mockResolvedValue([celeste, doom, hades]);
  vi.mocked(api.countGames).mockResolvedValue([3]);
  vi.mocked(api.listCategories).mockResolvedValue([coop, rpg]);
  await store.loadViews();
  await store.loadCategories();
  await store.refresh();
}

const tiles = () => screen.getAllByRole("gridcell").map((t) => t.getAttribute("aria-label"));

describe("grouping", () => {
  beforeEach(() => setUp("category"));

  it("shows a game once per category, under a header per group", () => {
    render(() => <GameGrid />);
    const headers = screen.getAllByRole("button", { expanded: true });
    expect(headers.map((h) => h.textContent)).toEqual([
      "Co-op1 game",
      "RPG2 games",
      "No category1 game",
    ]);
    expect(tiles()).toEqual(["Hades", "Celeste", "Hades", "Doom"]);
    expect(new Set(screen.getAllByRole("gridcell").map((t) => t.id)).size).toBe(4);
  });

  it("collapses a group and moves the cursor across groups", async () => {
    render(() => <GameGrid />);
    await fireEvent.click(screen.getByRole("button", { name: /^RPG/ }));
    expect(tiles()).toEqual(["Hades", "Doom"]);
    const grid = screen.getByRole("grid");
    await fireEvent.keyDown(grid, { key: "ArrowRight" });
    expect(store.selectedId()).toBe("h");
    await fireEvent.keyDown(grid, { key: "ArrowDown" });
    expect(store.selectedId()).toBe("d");
    expect(grid.getAttribute("aria-activedescendant")).toBe("tile-d::none");
  });

  it("groups the list too", () => {
    render(() => <GameList />);
    expect(screen.getAllByRole("option").map((o) => o.getAttribute("aria-label"))).toEqual([
      "Hades",
      "Celeste",
      "Hades",
      "Doom",
    ]);
  });
});

describe("toolbar", () => {
  beforeEach(() => setUp());

  it("groups the open view as an unsaved change", async () => {
    render(() => <TopBar />);
    await fireEvent.click(screen.getByRole("button", { name: "Group by" }));
    const menu = screen.getByRole("menu", { name: "Group by" });
    await fireEvent.click(within(menu).getByRole("menuitemcheckbox", { name: "Category" }));
    expect(store.groupBy()).toBe("category");
    expect(store.hasDraft("all")).toBe(true);
    expect(screen.getByRole("button", { name: "Grouped by category" })).toBeInTheDocument();
  });

  it("filters by category", async () => {
    render(() => <TopBar />);
    await fireEvent.click(screen.getByRole("button", { name: "Filters" }));
    await fireEvent.click(screen.getByRole("button", { name: "RPG" }));
    expect(store.query().categories).toEqual(["c1"]);
    await fireEvent.click(screen.getByRole("button", { name: "No category" }));
    expect(store.query().categories).toEqual(["c1", ":none"]);
    expect(screen.getByRole("button", { name: "Filters (1 active)" })).toBeInTheDocument();
  });
});

describe("CategoryEditor", () => {
  beforeEach(() => setUp());

  it("removes, adds and creates categories", async () => {
    vi.mocked(api.setGameCategories).mockImplementation(async (id, ids) => ({
      ...hades,
      id,
      categories: ids,
    }));
    vi.mocked(api.createCategory).mockResolvedValue(category("c3", "Roguelike", 0));
    render(() => <CategoryEditor game={hades} />);
    expect(screen.getAllByRole("listitem").map((li) => li.textContent)).toEqual([
      "Co-op",
      "RPG",
      "Category",
    ]);

    await fireEvent.click(screen.getByRole("button", { name: "Remove RPG" }));
    expect(api.setGameCategories).toHaveBeenLastCalledWith("h", ["c2"]);

    await fireEvent.click(screen.getByRole("button", { name: "Category" }));
    const input = screen.getByRole("combobox", { name: "Find or create a category" });
    await fireEvent.input(input, { target: { value: "rogue" } });
    expect(screen.getByRole("option", { name: "Create “rogue”" })).toBeInTheDocument();
    await fireEvent.keyDown(input, { key: "Enter" });
    await vi.waitFor(() => expect(api.createCategory).toHaveBeenCalledWith("rogue"));
    await vi.waitFor(() =>
      expect(api.setGameCategories).toHaveBeenLastCalledWith("h", ["c1", "c2", "c3"]),
    );

    await fireEvent.input(input, { target: { value: "co-op" } });
    expect(screen.queryByRole("option", { name: /Create/ })).toBeNull();
    await fireEvent.keyDown(input, { key: "Enter" });
    expect(api.setGameCategories).toHaveBeenLastCalledWith("h", ["c1"]);
  });
});

const rows = () =>
  screen
    .getAllByRole("listitem")
    .map((li) => li.querySelector(".category-list__name")?.textContent);

describe("CategoriesSection", () => {
  beforeEach(() => setUp());

  it("reorders with Alt+arrows", async () => {
    render(() => <CategoriesSection />);
    expect(rows()).toEqual(["Co-op", "RPG"]);
    screen.getByRole("button", { name: "Rename Co-op" }).focus();
    await fireEvent.keyDown(screen.getByRole("button", { name: "Rename Co-op" }), {
      key: "ArrowDown",
      altKey: true,
    });
    expect(api.reorderCategories).toHaveBeenCalledWith(["c1", "c2"]);
    expect(rows()).toEqual(["RPG", "Co-op"]);
    const rename = screen.getByRole("button", { name: "Rename Co-op" });
    expect(rename).toHaveFocus();
  });

  it("merges into another category after confirming", async () => {
    vi.mocked(api.mergeCategories).mockResolvedValue(rpg);
    render(() => <CategoriesSection />);
    await fireEvent.click(screen.getByRole("button", { name: "Merge Co-op into…" }));
    const menu = screen.getByRole("menu", { name: "Merge Co-op into" });
    await fireEvent.click(within(menu).getByRole("menuitem", { name: /RPG/ }));
    expect(api.mergeCategories).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole("button", { name: "Merge" }));
    expect(api.mergeCategories).toHaveBeenCalledWith("c2", "c1");
  });

  it("shows synced store categories and separates merged ones", async () => {
    vi.mocked(api.listCategories).mockResolvedValue([
      category("c1", "Playing", 3, [
        { source: "gog", name: "Currently playing" },
        { source: "steam", name: "playing" },
      ]),
    ]);
    await store.loadCategories();
    render(() => <CategoriesSection />);
    // Steam's "playing" is the category's own name: nothing to separate.
    expect(screen.getAllByRole("button", { name: /^Separate/ })).toHaveLength(1);
    await fireEvent.click(screen.getByRole("button", { name: /^Separate .*“Currently playing”$/ }));
    expect(api.separateCategorySource).toHaveBeenCalledWith("c1", "gog", "Currently playing");
  });
});
