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

const dialog = () => screen.getByRole("dialog", { name: "Add game" });
const askTitle = { name: "Unsaved changes" };

describe("Edit game dialog: unsaved changes", () => {
  it("closes at once on Escape and on a click outside when nothing was changed", async () => {
    const onClose = vi.fn<() => void>();
    render(() => <AddGameDialog onClose={onClose} />);
    await fireEvent.keyDown(dialog(), { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
    await fireEvent.mouseDown(dialog().parentElement as HTMLElement);
    expect(onClose).toHaveBeenCalledTimes(2);
    expect(screen.queryByRole("dialog", askTitle)).toBeNull();
  });

  it("asks before Escape or a click outside discards changes", async () => {
    const onClose = vi.fn<() => void>();
    render(() => <AddGameDialog onClose={onClose} />);
    await fireEvent.input(screen.getByRole("textbox", { name: "Name" }), {
      target: { value: "Mine" },
    });

    await fireEvent.keyDown(dialog(), { key: "Escape" });
    const ask = await screen.findByRole("dialog", askTitle);
    expect(onClose).not.toHaveBeenCalled();

    // Escape in the prompt only closes the prompt: keep editing.
    await fireEvent.keyDown(ask, { key: "Escape" });
    expect(screen.queryByRole("dialog", askTitle)).toBeNull();
    expect(screen.getByRole("textbox", { name: "Name" })).toHaveValue("Mine");

    await fireEvent.mouseDown(dialog().parentElement as HTMLElement);
    await fireEvent.click(await screen.findByRole("button", { name: "Keep editing" }));
    expect(screen.queryByRole("dialog", askTitle)).toBeNull();
    expect(onClose).not.toHaveBeenCalled();

    await fireEvent.keyDown(dialog(), { key: "Escape" });
    await fireEvent.click(await screen.findByRole("button", { name: "Discard" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("keeps Cancel discarding without asking", async () => {
    const onClose = vi.fn<() => void>();
    render(() => <AddGameDialog onClose={onClose} />);
    await fireEvent.input(screen.getByRole("textbox", { name: "Name" }), {
      target: { value: "Mine" },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("dialog", askTitle)).toBeNull();
  });

  it("saves from the prompt", async () => {
    vi.mocked(open).mockResolvedValue("C:\\Games\\Hades\\Hades.exe");
    const game = makeGame({ id: "g9" });
    vi.mocked(api.addManualGame).mockResolvedValue(game);
    vi.mocked(api.getGame).mockResolvedValue(game);
    vi.mocked(api.listGames).mockResolvedValue([game]);
    const onClose = vi.fn<() => void>();
    render(() => <AddGameDialog onClose={onClose} />);
    await fireEvent.click(screen.getByRole("button", { name: "Choose executable…" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Add game" })).toBeEnabled());
    await fireEvent.keyDown(dialog(), { key: "Escape" });
    await fireEvent.click(await screen.findByRole("button", { name: "Save" }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(api.addManualGame).toHaveBeenCalled();
  });
});
