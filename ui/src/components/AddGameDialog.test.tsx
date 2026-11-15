import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { open } from "@tauri-apps/plugin-dialog";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../lib/api";
import { store } from "../lib/store";
import { makeGame } from "../test-utils";
import { AddGameDialog } from "./AddGameDialog";

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

describe("AddGameDialog", () => {
  it("auto-fills the name from the picked executable", async () => {
    vi.mocked(open).mockResolvedValue("C:\\Games\\Cool_Game\\launcher.exe");
    render(() => <AddGameDialog onClose={vi.fn<() => void>()} />);
    await fireEvent.click(screen.getByRole("button", { name: "Choose executable…" }));
    await waitFor(() =>
      expect(screen.getByRole("textbox", { name: "Name" })).toHaveValue("Cool Game"),
    );
  });

  it("does not overwrite a name the user typed", async () => {
    vi.mocked(open).mockResolvedValue("C:\\Games\\Hades\\Hades.exe");
    render(() => <AddGameDialog onClose={vi.fn<() => void>()} />);
    await fireEvent.input(screen.getByRole("textbox", { name: "Name" }), {
      target: { value: "Mine" },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Choose executable…" }));
    await waitFor(() =>
      expect(screen.getByRole("textbox", { name: "Executable path" })).toHaveValue(
        "C:\\Games\\Hades\\Hades.exe",
      ),
    );
    expect(screen.getByRole("textbox", { name: "Name" })).toHaveValue("Mine");
  });

  it("submits and selects the new game", async () => {
    vi.mocked(open).mockResolvedValue("C:\\Games\\Hades\\Hades.exe");
    const game = makeGame({ id: "new" });
    vi.mocked(api.addManualGame).mockResolvedValue(game);
    vi.mocked(api.listGames).mockResolvedValue([game]);
    const onClose = vi.fn<() => void>();
    render(() => <AddGameDialog onClose={onClose} />);
    await fireEvent.click(screen.getByRole("button", { name: "Choose executable…" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Add game" })).toBeEnabled());
    await fireEvent.click(screen.getByRole("button", { name: "Add game" }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(api.addManualGame).toHaveBeenCalledWith({
      name: "Hades",
      executable: "C:\\Games\\Hades\\Hades.exe",
      args: null,
      workingDir: null,
      coverImage: null,
    });
    expect(store.selectedId()).toBe("new");
    expect(store.detailsOpen()).toBe(true);
  });

  it("fills in a picked installed program", async () => {
    vi.mocked(api.installedPrograms).mockResolvedValue([
      {
        name: "Celeste",
        executable: "C:\\Games\\Celeste\\Celeste.exe",
        args: "-fs",
        workingDir: "C:\\Games\\Celeste",
        origin: "startMenu",
      },
    ]);
    render(() => <AddGameDialog onClose={vi.fn<() => void>()} />);
    await fireEvent.click(screen.getByRole("button", { name: "Choose from installed programs…" }));
    await fireEvent.click(await screen.findByRole("button", { name: /Celeste/ }));
    expect(screen.getByRole("textbox", { name: "Executable path" })).toHaveValue(
      "C:\\Games\\Celeste\\Celeste.exe",
    );
    expect(screen.getByRole("textbox", { name: "Name" })).toHaveValue("Celeste");
    expect(screen.getByRole("textbox", { name: "Arguments" })).toHaveValue("-fs");
  });

  it("links the chosen match when the game is added", async () => {
    vi.mocked(open).mockResolvedValue("C:\\Games\\Hds\\hds.exe");
    vi.mocked(api.metadataSearchSources).mockResolvedValue([{ id: "steam", name: "Steam" }]);
    vi.mocked(api.searchMetadata).mockResolvedValue([
      { id: "1145360", name: "Hades", releaseYear: 2020, image: null },
    ]);
    const game = makeGame({ id: "new", source: "manual" });
    vi.mocked(api.addManualGame).mockResolvedValue(game);
    vi.mocked(api.setMetadataLink).mockResolvedValue(game);
    vi.mocked(api.listGames).mockResolvedValue([game]);
    render(() => <AddGameDialog onClose={vi.fn<() => void>()} />);
    await fireEvent.click(screen.getByRole("button", { name: "Choose executable…" }));
    await waitFor(() =>
      expect(screen.getByRole("textbox", { name: "Executable path" })).toHaveValue(
        "C:\\Games\\Hds\\hds.exe",
      ),
    );
    expect(screen.getByRole("textbox", { name: "Name" })).toHaveValue("hds");
    const search = await screen.findByRole("searchbox", { name: "Search for the game" });
    await fireEvent.input(search, { target: { value: "hades" } });
    await fireEvent.click(screen.getByRole("button", { name: "Search" }));
    await fireEvent.click(await screen.findByRole("button", { name: /Hades/ }));
    expect(api.searchMetadata).toHaveBeenCalledWith("steam", "hades");
    // The match's name replaces the one made from the file name.
    expect(screen.getByRole("textbox", { name: "Name" })).toHaveValue("Hades");
    await fireEvent.click(screen.getByRole("button", { name: "Add game" }));
    await waitFor(() =>
      expect(api.setMetadataLink).toHaveBeenCalledWith("new", {
        source: "steam",
        id: "1145360",
        name: "Hades",
      }),
    );
  });

  it("creates the game on leaving Setup and removes it on Cancel", async () => {
    vi.mocked(open).mockResolvedValue("C:\\Games\\Hades\\Hades.exe");
    const game = makeGame({ id: "new", name: "Hades", source: "manual" });
    vi.mocked(api.addManualGame).mockResolvedValue(game);
    vi.mocked(api.listGames).mockResolvedValue([game]);
    vi.mocked(api.removeGame).mockResolvedValue();
    const onClose = vi.fn<() => void>();
    render(() => <AddGameDialog onClose={onClose} />);

    // Without an executable the game can't be created yet.
    await fireEvent.click(screen.getByRole("button", { name: "Images" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Choose an executable first.");
    expect(api.addManualGame).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByRole("button", { name: "Choose executable…" }));
    await waitFor(() =>
      expect(screen.getByRole("textbox", { name: "Executable path" })).toHaveValue(
        "C:\\Games\\Hades\\Hades.exe",
      ),
    );
    await fireEvent.click(screen.getByRole("button", { name: "Images" }));
    await waitFor(() => expect(api.addManualGame).toHaveBeenCalled());
    // The full edit dialog, on the chosen section.
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Images" })).toHaveAttribute(
        "aria-current",
        "page",
      ),
    );
    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(api.removeGame).toHaveBeenCalledWith("new");
  });

  it("closes on Escape", async () => {
    const onClose = vi.fn<() => void>();
    render(() => <AddGameDialog onClose={onClose} />);
    await fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(onClose).toHaveBeenCalled();
  });
});
