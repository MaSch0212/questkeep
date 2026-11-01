import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ViewSection } from "../bindings/ViewSection";
import * as api from "../lib/api";
import { defaultQuery } from "../lib/query";
import { store } from "../lib/store";
import { makeGame } from "../test-utils";
import { HomePage } from "./HomePage";

vi.mock("../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));

const hades = makeGame({ id: "h", name: "Hades", sortingName: "hades" });

const sections: ViewSection[] = [
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
        group: "none",
        columns: [],
      },
    ],
  },
];

describe("HomePage", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    store.reset();
    vi.mocked(api.listViews).mockResolvedValue(sections);
    vi.mocked(api.listGames).mockResolvedValue([hades]);
    vi.mocked(api.countGames).mockResolvedValue([1]);
    vi.mocked(api.startPage).mockResolvedValue({
      recentlyPlayed: [{ game: hades, date: new Date().toISOString() }],
      recentlyAdded: [],
      playtime: [],
      stores: [],
    });
    await store.loadViews();
    store.showHome();
  });

  it("opens a game's details when it is clicked", async () => {
    render(() => <HomePage />);
    const tile = await screen.findByRole("gridcell", { name: "Hades" });
    await fireEvent.click(tile);
    expect(store.page()).toBe("library");
    expect(store.selectedId()).toBe("h");
    expect(store.detailsOpen()).toBe(true);
    await waitFor(() => expect(store.selectedGame()?.id).toBe("h"));
    expect(api.launchGame).not.toHaveBeenCalled();
  });
});
