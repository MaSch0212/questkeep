import { fireEvent, render, screen } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { makeGame } from "../test-utils";
import { GameTile } from "./GameTile";

vi.mock("../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => `asset://${p}`,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));

function renderTile(overrides = {}, onLaunch = vi.fn<(id: string) => void>()) {
  return render(() => (
    <GameTile
      game={makeGame(overrides)}
      selected={false}
      onSelect={vi.fn<(id: string) => void>()}
      onLaunch={onLaunch}
    />
  ));
}

function copy(id: string, source: string, installed: boolean) {
  return { id, source, sourceGameId: id, installed, playtimeSeconds: 0, lastPlayed: null };
}

describe("GameTile", () => {
  it("shows a placeholder with initials when there is no cover", () => {
    const { container } = renderTile({ name: "Dark Souls" });
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector(".cover--placeholder")).toHaveTextContent("DS");
    expect(container.querySelector(".cover__name")).toHaveTextContent("Dark Souls");
  });

  it("shows a lazy cover image when coverPath is set", () => {
    const { container } = renderTile({ coverPath: "C:\\covers\\a.jpg" });
    const img = container.querySelector("img");
    expect(img).not.toBeNull();
    expect(img?.getAttribute("src")).toBe("asset://C:\\covers\\a.jpg?v=0");
    expect(img?.getAttribute("loading")).toBe("lazy");
  });

  it("falls back to the placeholder when the image fails", async () => {
    const { container } = renderTile({ coverPath: "C:\\covers\\a.jpg" });
    const img = container.querySelector("img");
    if (img) await fireEvent.error(img);
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector(".cover--placeholder")).not.toBeNull();
  });

  it("shows the favorite badge and launches on double click", async () => {
    const onLaunch = vi.fn<(id: string) => void>();
    renderTile({ favorite: true }, onLaunch);
    expect(screen.getByTitle("Favorite")).toBeInTheDocument();
    await fireEvent.dblClick(screen.getByRole("gridcell", { name: "Hades" }));
    expect(onLaunch).toHaveBeenCalledWith("g1");
  });

  it("dims not-installed games, marks them with an icon and still reports the double click", async () => {
    const onLaunch = vi.fn<(id: string) => void>();
    renderTile({ installed: false }, onLaunch);
    const tile = screen.getByRole("gridcell", { name: "Hades (not installed)" });
    expect(tile).toHaveClass("tile--missing");
    expect(screen.getByTitle("Not installed").querySelector("svg")).not.toBeNull();
    await fireEvent.dblClick(tile);
    expect(onLaunch).toHaveBeenCalledWith("g1");
  });

  it("marks hidden games and shows an optional date on the cover", () => {
    const { container } = render(() => (
      <GameTile
        game={makeGame({ hidden: true })}
        selected={false}
        onSelect={vi.fn<(id: string) => void>()}
        onLaunch={vi.fn<(id: string) => void>()}
        date="Yesterday"
      />
    ));
    expect(screen.getByTitle("Hidden")).toBeInTheDocument();
    expect(container.querySelector(".tile__date")).toHaveTextContent("Yesterday");
  });

  it("names the primary store and marks a game that is in more stores", () => {
    const base = makeGame({ source: "steam", installed: true });
    const { container } = renderTile({
      copies: [copy("a", "steam", false), copy("b", "gog", true)],
      installed: base.installed,
    });
    // GOG's copy plays (the first installed one), so it is named and listed first.
    expect(container.querySelector(".tile__source-name")).toHaveTextContent("Gog");
    expect(container.querySelector(".tile__more")).not.toBeNull();
    expect(
      container.querySelectorAll(".tile__all .source-icon, .tile__all svg").length,
    ).toBeGreaterThan(0);
  });
});
