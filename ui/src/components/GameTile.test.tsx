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

describe("GameTile", () => {
  it("shows a placeholder with initials when there is no cover", () => {
    const { container } = renderTile({ name: "Dark Souls" });
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector(".cover--placeholder")).toHaveTextContent("DS");
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

  it("dims not-installed games, badges them and still reports the double click", async () => {
    const onLaunch = vi.fn<(id: string) => void>();
    const { container } = renderTile({ installed: false }, onLaunch);
    const tile = screen.getByRole("gridcell", { name: "Hades (not installed)" });
    expect(tile).toHaveClass("tile--missing");
    expect(container.querySelector(".badge--missing")).toHaveTextContent("Not installed");
    // Icon and text share one badge element.
    expect(container.querySelector(".badge--missing svg")).not.toBeNull();
    await fireEvent.dblClick(tile);
    expect(onLaunch).toHaveBeenCalledWith("g1");
  });
});
