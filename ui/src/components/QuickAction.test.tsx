import { fireEvent, render, screen } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../lib/api";
import { store } from "../lib/store";
import { makeGame } from "../test-utils";
import { GameRow } from "./GameRow";
import { GameTile } from "./GameTile";

vi.mock("../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => `asset://${p}`,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn<() => void>() }));

beforeEach(() => {
  vi.clearAllMocks();
  store.reset();
  vi.mocked(api.launchGame).mockResolvedValue(undefined);
  vi.mocked(api.installGame).mockResolvedValue(undefined);
});

function renderTile(overrides = {}, selected = false) {
  const onSelect = vi.fn<(id: string) => void>();
  const onLaunch = vi.fn<(id: string) => void>();
  render(() => (
    <GameTile
      game={makeGame(overrides)}
      selected={selected}
      onSelect={onSelect}
      onLaunch={onLaunch}
    />
  ));
  return { onSelect, onLaunch };
}

describe("quick action on a tile", () => {
  it("plays an installed game without selecting the tile or launching twice", async () => {
    const { onSelect, onLaunch } = renderTile();
    await fireEvent.click(screen.getByRole("button", { name: "Play Hades" }));
    expect(api.launchGame).toHaveBeenCalledWith("g1");
    expect(onSelect).not.toHaveBeenCalled();
    // A double click on the button must not also trigger the tile's own double-click launch.
    await fireEvent.dblClick(screen.getByRole("button", { name: "Play Hades" }));
    expect(onLaunch).not.toHaveBeenCalled();
  });

  it("installs a store game that is not installed", async () => {
    const { onSelect } = renderTile({ source: "steam", sourceGameId: "440", installed: false });
    expect(screen.queryByRole("button", { name: "Play Hades" })).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "Install Hades" }));
    expect(api.installGame).toHaveBeenCalledWith("g1");
    expect(api.launchGame).not.toHaveBeenCalled();
    expect(onSelect).not.toHaveBeenCalled();
  });

  it("offers no button for a missing manual game", () => {
    renderTile({ installed: false, sourceGameId: null });
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("shows the running state instead of Play and never launches twice", async () => {
    store.handleCoreEvent({ type: "gameStarted", id: "g1" });
    renderTile();
    expect(screen.queryByRole("button", { name: "Play Hades" })).toBeNull();
    const running = screen.getByRole("button", { name: "Hades is running" });
    expect(running).toBeDisabled();
    await fireEvent.click(running);
    expect(api.launchGame).not.toHaveBeenCalled();
  });

  it("is only in the tab order on the selected tile", () => {
    renderTile({}, false);
    expect(screen.getByRole("button", { name: "Play Hades" })).toHaveAttribute("tabindex", "-1");
  });

  it("is tabbable on the selected tile and keeps Enter away from the library key handler", async () => {
    const outer = vi.fn<(event: KeyboardEvent) => void>();
    render(() => (
      <div onKeyDown={outer}>
        <GameTile
          game={makeGame()}
          selected
          onSelect={vi.fn<() => void>()}
          onLaunch={vi.fn<() => void>()}
        />
      </div>
    ));
    const button = screen.getByRole("button", { name: "Play Hades" });
    expect(button).toHaveAttribute("tabindex", "0");
    await fireEvent.keyDown(button, { key: "Enter" });
    expect(outer).not.toHaveBeenCalled();
    await fireEvent.keyDown(button, { key: "ArrowRight" });
    expect(outer).toHaveBeenCalledTimes(1);
  });
});

describe("quick action on a row", () => {
  it("plays from the list row without selecting it", async () => {
    const onSelect = vi.fn<(id: string) => void>();
    render(() => (
      <GameRow
        game={makeGame()}
        selected={false}
        onSelect={onSelect}
        onLaunch={vi.fn<() => void>()}
      />
    ));
    await fireEvent.click(screen.getByRole("button", { name: "Play Hades" }));
    expect(api.launchGame).toHaveBeenCalledWith("g1");
    expect(onSelect).not.toHaveBeenCalled();
  });
});

describe("store banner and icon", () => {
  it("shows the store name on the tile banner and in the list row", async () => {
    vi.mocked(api.listProviders).mockResolvedValue([
      { id: "steam", name: "Steam", canScan: true, canScanOwned: true, canUninstall: true },
    ]);
    const { listen } = await import("@tauri-apps/api/event");
    vi.mocked(listen).mockResolvedValue(() => undefined);
    vi.mocked(api.appInfo).mockResolvedValue({
      version: "0",
      dataDir: "",
      logsDir: "",
      portable: false,
      devTools: false,
    });
    vi.mocked(api.runningGames).mockResolvedValue([]);
    vi.mocked(api.listGames).mockResolvedValue([]);
    await store.init();
    const game = makeGame({ source: "steam", sourceGameId: "1" });
    const { container } = render(() => (
      <>
        <GameTile
          game={game}
          selected={false}
          onSelect={vi.fn<() => void>()}
          onLaunch={vi.fn<() => void>()}
        />
        <GameRow
          game={game}
          selected={false}
          onSelect={vi.fn<() => void>()}
          onLaunch={vi.fn<() => void>()}
        />
      </>
    ));
    expect(container.querySelector(".tile__source")).toHaveTextContent("Steam");
    expect(container.querySelector(".tile__source svg")).not.toBeNull();
    expect(container.querySelector(".list-row__source")).toHaveTextContent("Steam");
    expect(container.querySelector(".list-row__source svg")).not.toBeNull();
  });

  it("falls back to a generic glyph and the id for an unknown source", () => {
    const { container } = render(() => (
      <GameTile
        game={makeGame({ source: "itch" })}
        selected={false}
        onSelect={vi.fn<() => void>()}
        onLaunch={vi.fn<() => void>()}
      />
    ));
    expect(container.querySelector(".tile__source")).toHaveTextContent("itch");
    expect(container.querySelector(".tile__source svg")).not.toBeNull();
  });
});
