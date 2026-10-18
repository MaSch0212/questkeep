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

  it("toggles favorite", async () => {
    vi.mocked(api.updateGame).mockResolvedValue(makeGame({ favorite: true }));
    render(() => <DetailsPanel game={makeGame()} onClose={vi.fn<() => void>()} />);
    await fireEvent.click(screen.getByRole("checkbox", { name: "Favorite" }));
    await waitFor(() =>
      expect(api.updateGame).toHaveBeenCalledWith(
        "g1",
        expect.objectContaining({ favorite: true, name: null }),
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
});
