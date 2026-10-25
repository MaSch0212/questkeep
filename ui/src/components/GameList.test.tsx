import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../lib/api";
import { store } from "../lib/store";
import { makeGame } from "../test-utils";
import { GameList } from "./GameList";

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
});

describe("GameList", () => {
  it("only renders the rows near the viewport", async () => {
    const games = Array.from({ length: 5000 }, (_, i) =>
      makeGame({ id: `g${i}`, name: `Game ${i}` }),
    );
    await load(games);
    render(() => <GameList />);
    const rows = screen.getAllByRole("option");
    expect(rows.length).toBeGreaterThan(5);
    expect(rows.length).toBeLessThan(40);
    expect(screen.getByRole("option", { name: "Game 0" })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: "Game 4999" })).toBeNull();
    // The canvas keeps the full scroll height.
    expect(document.querySelector<HTMLElement>(".list__canvas")?.style.height).toBe(
      `${5000 * 36 + 16}px`,
    );
  });

  it("shows source, state, playtime and release year", async () => {
    await load([makeGame({ name: "Hades", playtimeSeconds: 3600 * 2, releaseDate: "2020-09-17" })]);
    render(() => <GameList />);
    const row = screen.getByRole("option", { name: "Hades" });
    expect(row).toHaveTextContent("Installed");
    expect(row).toHaveTextContent("2h");
    expect(row).toHaveTextContent("2020");
  });

  it("selects and opens details on click, and moves with the arrow keys", async () => {
    await load([makeGame({ id: "a", name: "Alpha" }), makeGame({ id: "b", name: "Beta" })]);
    render(() => <GameList />);
    await fireEvent.click(screen.getByRole("option", { name: "Alpha" }));
    expect(store.selectedId()).toBe("a");
    expect(store.detailsOpen()).toBe(true);
    await fireEvent.keyDown(screen.getByRole("listbox", { name: "Games" }), { key: "ArrowDown" });
    expect(store.selectedId()).toBe("b");
    expect(screen.getByRole("option", { name: "Beta" })).toHaveAttribute("aria-selected", "true");
  });

  it("launches installed games on double click", async () => {
    vi.mocked(api.launchGame).mockResolvedValue(undefined);
    await load();
    render(() => <GameList />);
    await fireEvent.dblClick(screen.getByRole("option", { name: "Hades" }));
    expect(api.launchGame).toHaveBeenCalledWith("g1");
    expect(api.installGame).not.toHaveBeenCalled();
  });

  it("installs not-installed games on double click and Ctrl+Enter", async () => {
    vi.mocked(api.installGame).mockResolvedValue(undefined);
    await load([makeGame({ installed: false })]);
    render(() => <GameList />);
    const row = screen.getByRole("option", { name: "Hades (not installed)" });
    expect(row).toHaveTextContent("Not installed");
    await fireEvent.dblClick(row);
    await waitFor(() => expect(api.installGame).toHaveBeenCalledTimes(1));
    expect(api.launchGame).not.toHaveBeenCalled();

    store.selectGame("g1");
    await fireEvent.keyDown(screen.getByRole("listbox", { name: "Games" }), {
      key: "Enter",
      ctrlKey: true,
    });
    await waitFor(() => expect(api.installGame).toHaveBeenCalledTimes(2));
    expect(api.installGame).toHaveBeenCalledWith("g1");
  });
});
