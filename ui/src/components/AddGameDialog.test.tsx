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

  it("closes on Escape", async () => {
    const onClose = vi.fn<() => void>();
    render(() => <AddGameDialog onClose={onClose} />);
    await fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(onClose).toHaveBeenCalled();
  });
});
