import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Game } from "../bindings/Game";
import * as api from "../lib/api";
import { copiesOf, playCopy, storesOf } from "../lib/copies";
import { groupGames } from "../lib/grouping";
import { defaultQuery } from "../lib/query";
import { store } from "../lib/store";
import { matchesFilters } from "../lib/views";
import { makeGame } from "../test-utils";
import { DetailsPanel } from "./DetailsPanel";
import { DuplicatesDialog } from "./DuplicatesDialog";
import { GameRow } from "./GameRow";
import { GameRowCompact } from "./GameRowCompact";
import { GameTile } from "./GameTile";

vi.mock("../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn<() => void>() }));

beforeEach(() => {
  vi.clearAllMocks();
  store.reset();
});

const copy = (id: string, source: string, installed: boolean, playtimeSeconds = 0) => ({
  id,
  source,
  sourceGameId: id,
  installed,
  playtimeSeconds,
  lastPlayed: null,
});

/** Rocket League in Steam (not installed, first) and Epic (installed). */
const rocket = (overrides: Partial<Game> = {}) =>
  makeGame({
    id: "rl",
    name: "Rocket League",
    source: "steam",
    sourceGameId: "252950",
    installed: true,
    copies: [copy("rl", "steam", false, 3600), copy("rl-epic", "epic", true)],
    ...overrides,
  });

const noop = () => undefined;

/** One game in Steam and Epic, as two library entries. */
const group = (name: string, ids: [string, string]) => [
  makeGame({ id: ids[0], name, source: "steam" }),
  makeGame({ id: ids[1], name, source: "epic" }),
];

describe("copies", () => {
  it("lists stores once, plays the first installed copy and matches every store", () => {
    const game = rocket();
    expect(storesOf(game)).toEqual(["steam", "epic"]);
    expect(playCopy(game)?.id).toBe("rl-epic");
    expect(copiesOf(makeGame({ copies: [] }))).toHaveLength(1);
    const query = { ...defaultQuery(), sources: ["epic"] };
    expect(matchesFilters(query, game)).toBe(true);
    expect(matchesFilters({ ...query, sources: ["gog"] }, game)).toBe(false);
    const groups = groupGames([game], "source", [], (id) => id);
    expect(groups.map((g) => g.key)).toEqual(["epic", "steam"]);
  });

  it("counts a game as running while any of its copies runs", async () => {
    vi.mocked(api.listViews).mockResolvedValue([
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
          },
        ],
      },
    ]);
    vi.mocked(api.listGames).mockResolvedValue([rocket()]);
    vi.mocked(api.countGames).mockResolvedValue([1]);
    await store.loadViews();
    await store.refresh();
    expect(store.isRunning("rl")).toBe(false);
    store.handleCoreEvent({ type: "gameStarted", id: "rl-epic" });
    expect(store.isRunning("rl")).toBe(true);
  });
});

describe("store icons", () => {
  it("shows every store in the grid, the table and the compact list", () => {
    const game = rocket();
    render(() => (
      <>
        <GameTile game={game} selected={false} onSelect={noop} onLaunch={noop} />
        <GameRow game={game} selected={false} onSelect={noop} onLaunch={noop} />
        <GameRowCompact game={game} selected={false} onSelect={noop} />
      </>
    ));
    const groups = document.querySelectorAll(".source-icons");
    expect(groups).toHaveLength(3);
    for (const icons of groups) expect(icons.children).toHaveLength(2);
    expect(screen.getAllByTitle("Steam, Epic")).toHaveLength(3);
    // The compact row puts the icons after the name.
    const compact = document.querySelector(".compact-row");
    const children = [...(compact?.children ?? [])].map((el) => el.className);
    expect(children.indexOf("compact-row__source")).toBeGreaterThan(
      children.indexOf("compact-row__name"),
    );
  });
});

describe("details of a combined game", () => {
  it("plays with a chosen store, prefers and separates copies", async () => {
    const game = rocket();
    vi.mocked(api.preferCopy).mockResolvedValue(game);
    vi.mocked(api.separateCopy).mockResolvedValue(game);
    render(() => <DetailsPanel game={game} onClose={() => undefined} />);

    await fireEvent.click(screen.getByRole("button", { name: "Play" }));
    expect(api.launchGame).toHaveBeenCalledWith("rl", null);

    await fireEvent.click(screen.getByRole("button", { name: "Choose a store" }));
    const menu = screen.getByRole("menu", { name: "Stores" });
    expect(within(menu).getByRole("menuitem", { name: /Install with Steam/ })).toBeTruthy();
    await fireEvent.click(within(menu).getByRole("menuitem", { name: /Play with Epic/ }));
    await waitFor(() => expect(api.launchGame).toHaveBeenLastCalledWith("rl", "rl-epic"));

    const stores = screen.getByRole("region", { name: "Stores" });
    expect(within(stores).getByText(/Not installed · 1h/)).toBeTruthy();
    await fireEvent.click(within(stores).getByRole("button", { name: "Play with Epic first" }));
    expect(api.preferCopy).toHaveBeenCalledWith("rl-epic");
    await fireEvent.click(within(stores).getByRole("button", { name: "Separate Steam's copy" }));
    expect(api.separateCopy).toHaveBeenCalledWith("rl");
  });

  it("combines with a game picked by name", async () => {
    const steam = makeGame({ id: "s", name: "Rocket League", source: "steam" });
    const epic = makeGame({ id: "e", name: "Rocket League®", source: "epic" });
    vi.mocked(api.listGames).mockResolvedValue([steam, epic]);
    vi.mocked(api.combineGames).mockResolvedValue(rocket());
    render(() => <DetailsPanel game={steam} onClose={() => undefined} />);
    expect(screen.queryByRole("button", { name: "Choose a store" })).toBeNull();

    await fireEvent.click(screen.getByRole("button", { name: "Combine with another game…" }));
    const input = screen.getByRole("combobox", { name: "Find a game" });
    expect((input as HTMLInputElement).value).toBe("Rocket League");
    await waitFor(() => expect(screen.getAllByRole("option")).toHaveLength(1));
    expect(api.listGames).toHaveBeenCalledWith(
      expect.objectContaining({ search: "Rocket League", hidden: "all" }),
    );
    await fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(api.combineGames).toHaveBeenCalledWith("s", ["e"]));
  });
});

describe("DuplicatesDialog", () => {
  it("combines a group into its first game or marks it as different games", async () => {
    vi.mocked(api.findDuplicates).mockResolvedValue([
      group("Celeste", ["c1", "c2"]),
      group("Hades", ["h1", "h2"]),
    ]);
    vi.mocked(api.combineGames).mockResolvedValue(makeGame());
    render(() => <DuplicatesDialog onClose={() => undefined} />);
    await screen.findByRole("button", { name: "Combine Celeste" });
    expect(screen.getByRole("button", { name: "Combine all (2)" })).toBeTruthy();

    await fireEvent.click(screen.getByRole("button", { name: "Hades are different games" }));
    await waitFor(() => expect(api.dismissDuplicates).toHaveBeenCalledWith(["h1", "h2"]));
    await waitFor(() => expect(screen.queryByRole("button", { name: "Combine Hades" })).toBeNull());
    await fireEvent.click(screen.getByRole("button", { name: "Combine Celeste" }));
    await waitFor(() => expect(api.combineGames).toHaveBeenCalledWith("c1", ["c2"]));
    await screen.findByText("No games found that are in more than one store.");
  });
});
