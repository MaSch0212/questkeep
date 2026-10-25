import { fireEvent, render, screen, within } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { GameQuery } from "../bindings/GameQuery";
import type { LibraryView } from "../bindings/LibraryView";
import type { ViewSection } from "../bindings/ViewSection";
import * as api from "../lib/api";
import { defaultQuery } from "../lib/query";
import { store } from "../lib/store";
import { Sidebar } from "./Sidebar";

vi.mock("../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));

function view(id: string, name: string, query: Partial<GameQuery> = {}): LibraryView {
  return { id, name, icon: "star", query: { ...defaultQuery(), ...query }, layout: "grid" };
}

function sections(): ViewSection[] {
  return [
    {
      id: "library",
      name: "Library",
      views: [view("all", "All games"), view("favorites", "Favorites", { favorite: "yes" })],
    },
    { id: "other", name: "Other", views: [view("hidden", "Hidden", { hidden: "hidden" })] },
  ];
}

beforeEach(async () => {
  vi.clearAllMocks();
  store.reset();
  vi.mocked(api.listViews).mockResolvedValue(sections());
  vi.mocked(api.listGames).mockResolvedValue([]);
  vi.mocked(api.countGames).mockResolvedValue([12, 3, 1]);
  vi.mocked(api.arrangeViews).mockResolvedValue(undefined);
  vi.mocked(api.deleteViewSection).mockResolvedValue(undefined);
  await store.loadViews();
  await store.refresh();
});

describe("Sidebar", () => {
  it("lists the views by section with their game counts", async () => {
    render(() => <Sidebar />);
    expect(screen.getByRole("heading", { name: "Library" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Other" })).toBeInTheDocument();
    const all = screen.getByRole("button", { name: /All games/ });
    expect(all).toHaveAttribute("aria-current", "page");
    expect(all).toHaveTextContent("12");
    expect(screen.getByRole("button", { name: /Favorites/ })).toHaveTextContent("3");
  });

  it("opens a view and marks views with unsaved changes", async () => {
    render(() => <Sidebar />);
    await fireEvent.click(screen.getByRole("button", { name: /Favorites/ }));
    expect(store.activeViewId()).toBe("favorites");
    await vi.waitFor(() =>
      expect(vi.mocked(api.listGames).mock.lastCall?.[0]).toMatchObject({ favorite: "yes" }),
    );

    store.setViewMode("list");
    store.selectView("all");
    expect(screen.getByRole("button", { name: /Favorites/ })).toHaveTextContent(
      "(unsaved changes)",
    );
    expect(screen.getByRole("button", { name: /All games/ })).not.toHaveTextContent("unsaved");
  });

  it("renames from the context menu in edit mode", async () => {
    render(() => <Sidebar />);
    await fireEvent.contextMenu(screen.getByRole("button", { name: /Favorites/ }));
    const menu = screen.getByRole("menu", { name: "Favorites options" });
    await fireEvent.click(within(menu).getByRole("menuitem", { name: "Rename" }));
    expect(store.editingViews()).toBe(true);
    const names = screen.getAllByRole("textbox", { name: "View name" });
    expect(names.map((n) => (n as HTMLInputElement).value)).toEqual([
      "All games",
      "Favorites",
      "Hidden",
    ]);
  });

  it("asks before deleting a section that still has views", async () => {
    store.setEditingViews(true);
    render(() => <Sidebar />);
    await fireEvent.click(screen.getByRole("button", { name: "Delete section Other" }));
    expect(api.deleteViewSection).not.toHaveBeenCalled();
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Delete “Other” and its 1 view?");
    await fireEvent.click(within(alert).getByRole("button", { name: "Delete" }));
    expect(api.deleteViewSection).toHaveBeenCalledWith("other");
  });

  it("moves a view into the next section with Alt+Down", async () => {
    store.setEditingViews(true);
    render(() => <Sidebar />);
    const favorites = screen.getAllByRole("textbox", { name: "View name" })[1];
    await fireEvent.keyDown(favorites as HTMLElement, { key: "ArrowDown", altKey: true });
    expect(api.arrangeViews).toHaveBeenCalledWith([
      { id: "library", views: ["all"] },
      { id: "other", views: ["favorites", "hidden"] },
    ]);
  });
});
