import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../lib/api";
import { store } from "../lib/store";
import { makeGame } from "../test-utils";
import { DetailsView } from "./DetailsView";

vi.mock("../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => `asset://${p}`,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn<() => void>() }));

async function load(games = [makeGame()]) {
  vi.mocked(api.listGames).mockResolvedValue(games);
  await store.refresh();
}

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  store.reset();
  vi.mocked(api.launchGame).mockResolvedValue(undefined);
});

const page = () => screen.getByRole("complementary", { name: "Game details" });

describe("DetailsView", () => {
  it("selects the first game and shows its page without a close button", async () => {
    await load([
      makeGame({
        id: "a",
        name: "Alpha",
        source: "steam",
        sourceGameId: "1",
        playtimeSeconds: 3600 * 3,
        playCount: 7,
        developers: ["Supergiant"],
        publishers: ["Annapurna"],
        releaseDate: "2020-09-17",
        description: "A long descent.",
        genres: ["Roguelike"],
        backgroundPath: "C:\\bg\\a.jpg",
      }),
      makeGame({ id: "b", name: "Beta" }),
    ]);
    const { container } = render(() => <DetailsView />);
    await waitFor(() => expect(store.selectedId()).toBe("a"));
    expect(page()).toHaveClass("details--page");
    expect(page().querySelector("h2")).toHaveTextContent("Alpha");
    expect(screen.getByRole("button", { name: "Play" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Close details" })).toBeNull();
    expect(page()).toHaveTextContent("Supergiant");
    expect(page()).toHaveTextContent("Annapurna");
    expect(page()).toHaveTextContent("3h");
    expect(page()).toHaveTextContent("Play count");
    expect(page()).toHaveTextContent("A long descent.");
    expect(page()).toHaveTextContent("Roguelike");
    expect(page().querySelector(".facts__source")).toHaveTextContent("Steam");
    expect(container.querySelector(".details__hero img")).not.toBeNull();
    // The list highlights the selection.
    expect(screen.getByRole("option", { name: "Alpha" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("option", { name: "Beta" })).toHaveAttribute("aria-selected", "false");
  });

  it("keeps an existing selection and switches with a click or the arrow keys", async () => {
    await load([makeGame({ id: "a", name: "Alpha" }), makeGame({ id: "b", name: "Beta" })]);
    store.selectGame("b");
    render(() => <DetailsView />);
    expect(page().querySelector("h2")).toHaveTextContent("Beta");
    await fireEvent.click(screen.getByRole("option", { name: "Alpha" }));
    expect(store.selectedId()).toBe("a");
    expect(store.detailsOpen()).toBe(false);
    expect(page().querySelector("h2")).toHaveTextContent("Alpha");
    await fireEvent.keyDown(screen.getByRole("listbox", { name: "Games" }), { key: "ArrowDown" });
    expect(store.selectedId()).toBe("b");
    expect(page().querySelector("h2")).toHaveTextContent("Beta");
  });

  it("keeps the actions of the panel on the page", async () => {
    await load([makeGame({ source: "steam", sourceGameId: "1" })]);
    vi.mocked(api.listProviders).mockResolvedValue([
      {
        id: "steam",
        name: "Steam",
        icon: "<svg></svg>",
        canScan: true,
        canScanOwned: true,
        canUninstall: true,
        addGameOptions: [],
      },
    ]);
    render(() => <DetailsView />);
    await fireEvent.click(await screen.findByRole("button", { name: "Play" }));
    expect(api.launchGame).toHaveBeenCalledWith("g1", null);
    for (const name of [
      "Favorite",
      "Hidden",
      "Change cover…",
      "Refresh metadata",
      "Refetch artwork",
      "Remove from library",
    ]) {
      expect(screen.getByRole("button", { name })).toBeInTheDocument();
    }
  });

  it("only renders the rows near the viewport of the narrow list", async () => {
    await load(
      Array.from({ length: 5000 }, (_, i) => makeGame({ id: `g${i}`, name: `Game ${i}` })),
    );
    render(() => <DetailsView />);
    const rows = screen.getAllByRole("option");
    expect(rows.length).toBeGreaterThan(5);
    expect(rows.length).toBeLessThan(40);
    expect(document.querySelector<HTMLElement>(".list__canvas")?.style.height).toBe(
      `${5000 * 32 + 16}px`,
    );
    expect(screen.queryByRole("option", { name: "Game 4999" })).toBeNull();
  });

  it("shows a hint when the library is empty", async () => {
    await load([]);
    render(() => <DetailsView />);
    expect(screen.getByText("Select a game to see its details.")).toBeInTheDocument();
  });
});
