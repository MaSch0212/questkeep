import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../lib/api";
import { store } from "../lib/store";
import { makeGame } from "../test-utils";
import { DetailsPanel } from "./DetailsPanel";

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
});

describe("DetailsPanel", () => {
  it("launches the game with Play", async () => {
    vi.mocked(api.launchGame).mockResolvedValue(undefined);
    render(() => <DetailsPanel game={makeGame()} onClose={vi.fn<() => void>()} />);
    await fireEvent.click(screen.getByRole("button", { name: "Play" }));
    expect(api.launchGame).toHaveBeenCalledWith("g1");
  });

  it("shows launch errors", async () => {
    vi.mocked(api.launchGame).mockRejectedValue("exe not found");
    render(() => <DetailsPanel game={makeGame()} onClose={vi.fn<() => void>()} />);
    await fireEvent.click(screen.getByRole("button", { name: "Play" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("exe not found");
  });

  it("disables Play while the game is running", () => {
    store.handleCoreEvent({ type: "gameStarted", id: "g1" });
    render(() => <DetailsPanel game={makeGame()} onClose={vi.fn<() => void>()} />);
    expect(screen.getByRole("button", { name: "Running…" })).toBeDisabled();
  });

  it("renames inline with Enter", async () => {
    vi.mocked(api.updateGame).mockResolvedValue(makeGame({ name: "Hades II" }));
    render(() => <DetailsPanel game={makeGame()} onClose={vi.fn<() => void>()} />);
    await fireEvent.click(screen.getByRole("button", { name: "Rename game" }));
    const input = screen.getByRole("textbox", { name: "Game name" });
    await fireEvent.input(input, { target: { value: "Hades II" } });
    await fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() =>
      expect(api.updateGame).toHaveBeenCalledWith("g1", {
        name: "Hades II",
        favorite: null,
        hidden: null,
        playAction: null,
      }),
    );
  });

  it("cancels renaming with Escape", async () => {
    render(() => <DetailsPanel game={makeGame()} onClose={vi.fn<() => void>()} />);
    await fireEvent.click(screen.getByRole("button", { name: "Rename game" }));
    await fireEvent.keyDown(screen.getByRole("textbox", { name: "Game name" }), { key: "Escape" });
    expect(api.updateGame).not.toHaveBeenCalled();
    expect(screen.getByRole("heading", { name: "Hades" })).toBeInTheDocument();
  });

  it("toggles favorite with an icon button", async () => {
    vi.mocked(api.updateGame).mockResolvedValue(makeGame({ favorite: true }));
    render(() => <DetailsPanel game={makeGame()} onClose={vi.fn<() => void>()} />);
    const button = screen.getByRole("button", { name: "Favorite" });
    expect(button).toHaveAttribute("aria-pressed", "false");
    await fireEvent.click(button);
    await waitFor(() =>
      expect(api.updateGame).toHaveBeenCalledWith(
        "g1",
        expect.objectContaining({ favorite: true, name: null }),
      ),
    );
  });

  it("toggles hidden and reflects the current state", async () => {
    vi.mocked(api.updateGame).mockResolvedValue(makeGame({ hidden: false }));
    render(() => <DetailsPanel game={makeGame({ hidden: true })} onClose={vi.fn<() => void>()} />);
    const button = screen.getByRole("button", { name: "Hidden" });
    expect(button).toHaveAttribute("aria-pressed", "true");
    await fireEvent.click(button);
    await waitFor(() =>
      expect(api.updateGame).toHaveBeenCalledWith(
        "g1",
        expect.objectContaining({ hidden: false, name: null }),
      ),
    );
  });

  it("requires a confirm step before removing", async () => {
    vi.mocked(api.removeGame).mockResolvedValue(undefined);
    render(() => <DetailsPanel game={makeGame()} onClose={vi.fn<() => void>()} />);
    await fireEvent.click(screen.getByRole("button", { name: "Remove from library" }));
    expect(api.removeGame).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole("button", { name: "Confirm remove" }));
    await waitFor(() => expect(api.removeGame).toHaveBeenCalledWith("g1"));
  });

  it("offers Install instead of Play for games that are not installed", async () => {
    vi.mocked(api.installGame).mockResolvedValue(undefined);
    render(() => (
      <DetailsPanel game={makeGame({ installed: false })} onClose={vi.fn<() => void>()} />
    ));
    expect(screen.queryByRole("button", { name: "Play" })).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "Install" }));
    expect(api.installGame).toHaveBeenCalledWith("g1");
    expect(api.launchGame).not.toHaveBeenCalled();
  });

  it("shows metadata with a formatted release date", () => {
    render(() => (
      <DetailsPanel
        game={makeGame({
          description: "First line.\nSecond line.",
          releaseDate: "2020-09-17",
          developers: ["Supergiant Games"],
          publishers: ["Supergiant Games", "Other Co"],
          genres: ["Roguelike", "Action"],
        })}
        onClose={vi.fn<() => void>()}
      />
    ));
    expect(screen.getByText(/First line./)).toHaveTextContent("Second line.");
    expect(screen.getByText("Released").nextElementSibling).toHaveTextContent(/2020/);
    expect(screen.getByText("Released").nextElementSibling).toHaveTextContent(/17/);
    expect(screen.getByText("Developer").nextElementSibling).toHaveTextContent("Supergiant Games");
    expect(screen.getByText("Publishers").nextElementSibling).toHaveTextContent(
      "Supergiant Games, Other Co",
    );
    const chips = screen.getByRole("list", { name: "Genres" });
    expect(chips).toHaveTextContent("Roguelike");
    expect(chips).toHaveTextContent("Action");
  });

  it("shows year-only and month-only release dates", () => {
    const { unmount } = render(() => (
      <DetailsPanel game={makeGame({ releaseDate: "1999" })} onClose={vi.fn<() => void>()} />
    ));
    expect(screen.getByText("Released").nextElementSibling).toHaveTextContent(/^1999$/);
    unmount();
    render(() => (
      <DetailsPanel game={makeGame({ releaseDate: "2011-11" })} onClose={vi.fn<() => void>()} />
    ));
    const text = screen.getByText("Released").nextElementSibling?.textContent ?? "";
    expect(text).toContain("2011");
    expect(text).not.toMatch(/1/);
  });

  it("clamps long descriptions behind Show more", async () => {
    render(() => (
      <DetailsPanel
        game={makeGame({ description: "Lorem ipsum ".repeat(60) })}
        onClose={vi.fn<() => void>()}
      />
    ));
    const more = screen.getByRole("button", { name: "Show more" });
    expect(more).toHaveAttribute("aria-expanded", "false");
    await fireEvent.click(more);
    expect(screen.getByRole("button", { name: "Show less" })).toHaveAttribute(
      "aria-expanded",
      "true",
    );
    expect(document.querySelector(".description__text--clamped")).toBeNull();
  });

  it("shows the background image when there is one", () => {
    const { container } = render(() => (
      <DetailsPanel
        game={makeGame({ backgroundPath: "C:\\bg\\a.jpg" })}
        onClose={vi.fn<() => void>()}
      />
    ));
    expect(container.querySelector(".details__hero img")?.getAttribute("src")).toBe(
      "asset://C:\\bg\\a.jpg?v=0",
    );
  });

  it("refreshes metadata for store games only", async () => {
    vi.mocked(api.refreshMetadata).mockResolvedValue(undefined);
    const { unmount } = render(() => (
      <DetailsPanel game={makeGame({ sourceGameId: "1145360" })} onClose={vi.fn<() => void>()} />
    ));
    await fireEvent.click(screen.getByRole("button", { name: "Refresh metadata" }));
    await waitFor(() => expect(api.refreshMetadata).toHaveBeenCalledWith("g1"));
    await waitFor(() =>
      expect(store.toasts().map((t) => t.message)).toContain("Refreshing metadata…"),
    );
    unmount();
    render(() => <DetailsPanel game={makeGame()} onClose={vi.fn<() => void>()} />);
    expect(screen.queryByRole("button", { name: "Refresh metadata" })).toBeNull();
  });
});
