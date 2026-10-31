import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
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

async function loadProviders(canUninstall: boolean) {
  vi.mocked(api.listProviders).mockResolvedValue([
    {
      id: "steam",
      name: "Steam",
      icon: "<svg></svg>",
      canScan: true,
      canScanOwned: true,
      canUninstall,
      installed: true,
      syncable: true,
      addGameOptions: [],
    },
  ]);
  vi.mocked(api.appInfo).mockResolvedValue({
    version: "0",
    dataDir: "",
    logsDir: "",
    portable: false,
    devTools: false,
  });
  vi.mocked(api.runningGames).mockResolvedValue([]);
  vi.mocked(api.listGames).mockResolvedValue([]);
  const { listen } = await import("@tauri-apps/api/event");
  vi.mocked(listen).mockResolvedValue(() => undefined);
  await store.init();
}

const steamGame = (overrides = {}) =>
  makeGame({ source: "steam", sourceGameId: "440", ...overrides });

/** Opens the "More actions" menu and returns it. */
async function openMore() {
  await fireEvent.click(screen.getByRole("button", { name: "More actions" }));
  return screen.getByRole("menu", { name: "More actions" });
}

async function chooseMore(name: string | RegExp) {
  const menu = await openMore();
  await fireEvent.click(within(menu).getByRole("menuitem", { name }));
}

/** Whether the More menu offers Uninstall; closes it again. */
async function offered() {
  const menu = await openMore();
  const item = within(menu).queryByRole("menuitem", { name: "Uninstall…" });
  await fireEvent.keyDown(menu, { key: "Escape" });
  return item !== null;
}

describe("DetailsPanel", () => {
  it("launches the game with Play", async () => {
    vi.mocked(api.launchGame).mockResolvedValue(undefined);
    render(() => <DetailsPanel game={makeGame()} onClose={vi.fn<() => void>()} />);
    await fireEvent.click(screen.getByRole("button", { name: "Play" }));
    expect(api.launchGame).toHaveBeenCalledWith("g1", null);
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

  it("renames in the Edit dialog, opened with the button or F2", async () => {
    vi.mocked(api.updateGame).mockResolvedValue(makeGame({ name: "Hades II" }));
    render(() => <DetailsPanel game={makeGame()} onClose={vi.fn<() => void>()} />);
    await fireEvent.keyDown(window, { key: "F2" });
    const dialog = screen.getByRole("dialog", { name: "Edit game" });
    const input = within(dialog).getByRole("textbox", { name: "Name" });
    await waitFor(() => expect(input).toHaveFocus());
    await fireEvent.input(input, { target: { value: "Hades II" } });
    await fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(api.updateGame).toHaveBeenCalledWith("g1", {
        name: "Hades II",
        favorite: null,
        hidden: null,
        detectLaunches: null,
        ignoredProcesses: null,
        playAction: null,
      }),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("saves playtime tracking settings from the Edit dialog", async () => {
    vi.mocked(api.updateGame).mockResolvedValue(makeGame());
    render(() => <DetailsPanel game={makeGame()} onClose={vi.fn<() => void>()} />);
    await fireEvent.keyDown(window, { key: "F2" });
    const dialog = screen.getByRole("dialog", { name: "Edit game" });
    await fireEvent.click(
      within(dialog).getByRole("checkbox", {
        name: "Record playtime when started outside Questkeep",
      }),
    );
    await fireEvent.input(within(dialog).getByRole("textbox", { name: "Ignored programs" }), {
      target: { value: " Launcher.exe \n\n*patcher*" },
    });
    await fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    const updates = vi.mocked(api.updateGame).mock.calls.map(([, update]) => update);
    expect(updates).toContainEqual(expect.objectContaining({ detectLaunches: false }));
    expect(updates).toContainEqual(
      expect.objectContaining({ ignoredProcesses: ["Launcher.exe", "*patcher*"] }),
    );
  });

  it("discards the edits on Cancel and saves nothing when nothing changed", async () => {
    render(() => <DetailsPanel game={makeGame()} onClose={vi.fn<() => void>()} />);
    await fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    let dialog = screen.getByRole("dialog", { name: "Edit game" });
    await fireEvent.input(within(dialog).getByRole("textbox", { name: "Name" }), {
      target: { value: "Other" },
    });
    await fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByRole("heading", { name: "Hades" })).toBeInTheDocument();

    await fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    dialog = screen.getByRole("dialog", { name: "Edit game" });
    expect(within(dialog).getByRole("textbox", { name: "Name" })).toHaveValue("Hades");
    await fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(api.updateGame).not.toHaveBeenCalled();
    expect(api.setCover).not.toHaveBeenCalled();
  });

  it("applies a picked cover only on Save, and keeps the dialog open on errors", async () => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    vi.mocked(open).mockResolvedValue("C:\\img\\new.png");
    vi.mocked(api.setCover).mockRejectedValueOnce("Unsupported image");
    render(() => <DetailsPanel game={makeGame()} onClose={vi.fn<() => void>()} />);
    await fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    const dialog = screen.getByRole("dialog", { name: "Edit game" });
    await fireEvent.click(within(dialog).getByRole("button", { name: "Change…" }));
    expect(await within(dialog).findByText("New: new.png")).toBeTruthy();
    expect(api.setCover).not.toHaveBeenCalled();
    await fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Unsupported image");
    expect(api.setCover).toHaveBeenCalledWith("g1", "C:\\img\\new.png");
  });

  it("edits launch options of manually added games only", async () => {
    vi.mocked(api.updateGame).mockResolvedValue(makeGame());
    const { unmount } = render(() => (
      <DetailsPanel game={steamGame()} onClose={vi.fn<() => void>()} />
    ));
    await fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    expect(screen.queryByRole("group", { name: "Launch options" })).toBeNull();
    unmount();

    render(() => <DetailsPanel game={makeGame()} onClose={vi.fn<() => void>()} />);
    await fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    const options = screen.getByRole("group", { name: "Launch options" });
    await fireEvent.input(within(options).getByRole("textbox", { name: "Arguments" }), {
      target: { value: "-windowed" },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(api.updateGame).toHaveBeenCalledWith(
        "g1",
        expect.objectContaining({
          name: null,
          playAction: expect.objectContaining({ args: "-windowed" }),
        }),
      ),
    );
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
    await chooseMore("Remove from library…");
    expect(api.removeGame).not.toHaveBeenCalled();
    const dialog = screen.getByRole("dialog", { name: "Remove game" });
    await fireEvent.click(within(dialog).getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(api.removeGame).toHaveBeenCalledWith("g1"));
  });

  it("keeps an open confirmation when the list refreshes the same game", async () => {
    const [game, setGame] = createSignal(makeGame());
    render(() => <DetailsPanel game={game()} onClose={vi.fn<() => void>()} />);
    await chooseMore("Remove from library…");
    // A refresh: a new object for the same game.
    setGame(makeGame({ playtimeSeconds: 60 }));
    expect(screen.getByRole("dialog", { name: "Remove game" })).toBeTruthy();
    // Another game: the confirmation closes.
    setGame(makeGame({ id: "g2" }));
    expect(screen.queryByRole("dialog", { name: "Remove game" })).toBeNull();
  });

  describe("Uninstall", () => {
    it("is offered for installed games of providers that support it", async () => {
      await loadProviders(true);
      render(() => <DetailsPanel game={steamGame()} onClose={vi.fn<() => void>()} />);
      expect(await offered()).toBe(true);
    });

    it("is hidden for unsupported providers, manual games and uninstalled games", async () => {
      await loadProviders(false);
      const { unmount } = render(() => (
        <DetailsPanel game={steamGame()} onClose={vi.fn<() => void>()} />
      ));
      expect(await offered()).toBe(false);
      unmount();
      await loadProviders(true);
      const manual = render(() => <DetailsPanel game={makeGame()} onClose={vi.fn<() => void>()} />);
      expect(await offered()).toBe(false);
      manual.unmount();
      render(() => (
        <DetailsPanel game={steamGame({ installed: false })} onClose={vi.fn<() => void>()} />
      ));
      expect(await offered()).toBe(false);
    });

    it("asks for confirmation, focuses Cancel and only then uninstalls", async () => {
      vi.mocked(api.uninstallGame).mockResolvedValue(undefined);
      await loadProviders(true);
      render(() => <DetailsPanel game={steamGame()} onClose={vi.fn<() => void>()} />);
      await chooseMore("Uninstall…");
      expect(api.uninstallGame).not.toHaveBeenCalled();
      const dialog = screen.getByRole("dialog", { name: "Uninstall game" });
      expect(
        within(dialog).getByText(/Uninstall “Hades”\? The store will ask to confirm\./),
      ).toBeVisible();
      await waitFor(() =>
        expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus(),
      );
      await fireEvent.click(within(dialog).getByRole("button", { name: "Uninstall" }));
      await waitFor(() => expect(api.uninstallGame).toHaveBeenCalledWith("g1", "g1"));
    });

    it("cancels with the Cancel button or Escape", async () => {
      await loadProviders(true);
      render(() => <DetailsPanel game={steamGame()} onClose={vi.fn<() => void>()} />);
      await chooseMore("Uninstall…");
      await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
      expect(screen.queryByRole("dialog")).toBeNull();
      await chooseMore("Uninstall…");
      await fireEvent.keyDown(screen.getByRole("button", { name: "Cancel" }), { key: "Escape" });
      expect(screen.queryByRole("dialog")).toBeNull();
      expect(api.uninstallGame).not.toHaveBeenCalled();
    });

    it("shows an error when the uninstall can't be started", async () => {
      vi.mocked(api.uninstallGame).mockRejectedValue("Could not start the uninstall");
      await loadProviders(true);
      render(() => <DetailsPanel game={steamGame()} onClose={vi.fn<() => void>()} />);
      await chooseMore("Uninstall…");
      await fireEvent.click(screen.getByRole("button", { name: "Uninstall" }));
      expect(await screen.findByRole("alert")).toHaveTextContent("Could not start the uninstall");
    });
  });

  it("offers Install instead of Play for games that are not installed", async () => {
    vi.mocked(api.installGame).mockResolvedValue(undefined);
    render(() => (
      <DetailsPanel game={makeGame({ installed: false })} onClose={vi.fn<() => void>()} />
    ));
    expect(screen.queryByRole("button", { name: "Play" })).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "Install" }));
    expect(api.installGame).toHaveBeenCalledWith("g1", null);
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

  it("refreshes metadata, or shows why it can't", async () => {
    vi.mocked(api.refreshMetadata).mockResolvedValueOnce(undefined);
    const { unmount } = render(() => (
      <DetailsPanel game={makeGame({ sourceGameId: "1145360" })} onClose={vi.fn<() => void>()} />
    ));
    await chooseMore("Refresh metadata");
    await waitFor(() => expect(api.refreshMetadata).toHaveBeenCalledWith("g1"));
    await waitFor(() =>
      expect(store.toasts().map((t) => t.message)).toContain("Refreshing metadata…"),
    );
    unmount();
    // Manual games have the button too; without IGDB the core explains what is missing.
    const reason = "Add IGDB credentials in Settings → Metadata to get metadata for this game.";
    vi.mocked(api.refreshMetadata).mockRejectedValueOnce(reason);
    render(() => <DetailsPanel game={makeGame()} onClose={vi.fn<() => void>()} />);
    await chooseMore("Refresh metadata");
    await waitFor(() => expect(screen.getByText(reason)).toBeTruthy());
  });

  it("refetches artwork and toasts, or toasts the error", async () => {
    vi.mocked(api.refetchArtwork).mockResolvedValueOnce(undefined);
    render(() => <DetailsPanel game={makeGame()} onClose={vi.fn<() => void>()} />);
    await chooseMore("Refetch artwork");
    await waitFor(() => expect(api.refetchArtwork).toHaveBeenCalledWith("g1"));
    await waitFor(() =>
      expect(store.toasts().map((t) => t.message)).toContain("Fetching artwork…"),
    );
    vi.mocked(api.refetchArtwork).mockRejectedValueOnce(
      "Add a SteamGridDB API key in the settings first.",
    );
    await chooseMore("Refetch artwork");
    await waitFor(() =>
      expect(store.toasts().map((t) => t.message)).toContain(
        "Add a SteamGridDB API key in the settings first.",
      ),
    );
  });

  it("opens the install folder from the More menu when it is known", async () => {
    vi.mocked(api.openInstallFolder).mockResolvedValue(undefined);
    const { unmount } = render(() => (
      <DetailsPanel game={makeGame({ installDir: null })} onClose={vi.fn<() => void>()} />
    ));
    const menu = await openMore();
    expect(within(menu).queryByRole("menuitem", { name: "Open install folder" })).toBeNull();
    unmount();
    render(() => (
      <DetailsPanel
        game={makeGame({ installDir: "C:\\Games\\Hades" })}
        onClose={vi.fn<() => void>()}
      />
    ));
    await chooseMore("Open install folder");
    await waitFor(() => expect(api.openInstallFolder).toHaveBeenCalledWith("g1"));
  });
});
