import { fireEvent, render, screen, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { CoreEvent } from "../bindings/CoreEvent";
import type { ProviderInfo } from "../bindings/ProviderInfo";
import { makeGame } from "../test-utils";
import * as api from "../lib/api";
import { store } from "../lib/store";
import { TitleBar } from "./TitleBar";

vi.mock("../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));

function provider(id: string, name: string, canScan = true): ProviderInfo {
  return {
    id,
    name,
    icon: "<svg></svg>",
    canScan,
    canScanOwned: false,
    canUninstall: false,
    launcherPages: false,
    installed: true,
    syncable: canScan,
    addGameOptions: id === "prism" ? [{ id: "new-instance", label: "New Minecraft instance" }] : [],
    settings: { options: [], credentials: null },
  };
}

const send = (event: CoreEvent) => store.handleCoreEvent(event);
const scanned = (id: string, added: number, updated: number): CoreEvent => ({
  type: "libraryScanned",
  provider: id,
  summary: { added, updated, uninstalled: 0 },
});

beforeEach(async () => {
  vi.clearAllMocks();
  store.reset();
  vi.mocked(api.listProviders).mockResolvedValue([
    provider("manual", "Manual", false),
    provider("steam", "Steam"),
    provider("epic", "Epic Games"),
    provider("prism", "Prism Launcher"),
  ]);
  vi.mocked(api.listGames).mockResolvedValue([]);
  vi.mocked(api.listViews).mockResolvedValue([]);
  vi.mocked(api.countGames).mockResolvedValue([]);
  vi.mocked(api.appInfo).mockResolvedValue({
    version: "0",
    dataDir: "",
    logsDir: "",
    portable: false,
    devTools: false,
  });
  vi.mocked(api.runningGames).mockResolvedValue([]);
  vi.mocked(api.syncTimes).mockResolvedValue({
    all: null,
    providers: { steam: new Date(Date.now() - 12 * 60_000).toISOString() },
  });
  vi.mocked(api.listLaunchers).mockResolvedValue(["steam"]);
  vi.mocked(api.scanAllLibraries).mockResolvedValue(undefined);
  vi.mocked(api.scanLibrary).mockResolvedValue({ added: 0, updated: 0, uninstalled: 0 });
  vi.mocked(api.openLauncher).mockResolvedValue(undefined);
  const { listen } = await import("@tauri-apps/api/event");
  vi.mocked(listen).mockResolvedValue(() => undefined);
  await store.init();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("Sync", () => {
  it("spins while a run is busy and names the progress", () => {
    render(() => <TitleBar onAdd={vi.fn<() => void>()} />);
    expect(screen.getByRole("button", { name: "Sync" }).querySelector(".icon--spin")).toBeNull();
    send({ type: "scansQueued", providers: ["steam", "epic"] });
    send({ type: "scanStarted", provider: "steam" });
    const busy = screen.getByRole("button", { name: "Syncing 1 of 2…" });
    expect(busy.querySelector(".icon--spin")).not.toBeNull();
  });

  it("always lists every store that can sync, with its status", async () => {
    render(() => <TitleBar onAdd={vi.fn<() => void>()} />);
    await fireEvent.click(screen.getByRole("button", { name: "Sync" }));
    const dialog = screen.getByRole("dialog", { name: "Library sync" });
    const rows = dialog.querySelectorAll("li");
    expect([...rows].map((r) => r.querySelector(".sync-panel__name")?.textContent)).toEqual([
      "Steam",
      "Epic Games",
      "Prism Launcher",
    ]);
    expect(rows[0]).toHaveTextContent("Synced 12 min ago");
    expect(rows[1]).toHaveTextContent("Not synced yet");
    expect(dialog).toHaveTextContent("Last full sync: never");
    await fireEvent.click(within(dialog).getByRole("button", { name: "Sync Steam" }));
    expect(api.scanLibrary).toHaveBeenCalledWith("steam");
    await fireEvent.click(within(dialog).getByRole("button", { name: /Sync all/ }));
    expect(api.scanAllLibraries).toHaveBeenCalledTimes(1);

    send({ type: "scanStarted", provider: "epic" });
    expect(rows[1]).toHaveTextContent("Scanning…");
    send({ type: "scanFailed", provider: "epic", message: "No network" });
    expect(rows[1]).toHaveTextContent("No network");
  });

  it("shows one toast for the whole run", () => {
    vi.useFakeTimers();
    render(() => <TitleBar onAdd={vi.fn<() => void>()} />);
    send({ type: "scansQueued", providers: ["steam", "epic"] });
    send({ type: "scanStarted", provider: "steam" });
    send(scanned("steam", 3, 1));
    send({ type: "scanStarted", provider: "epic" });
    send({ type: "scanFailed", provider: "epic", message: "No network" });
    vi.advanceTimersByTime(1500);
    expect(store.toasts().map((t) => t.message)).toEqual([
      "Sync finished: 3 added, 1 updated, 1 failed",
    ]);
  });
});

describe("Menus", () => {
  it("adds games from the Add game menu", async () => {
    const onAdd = vi.fn<() => void>();
    render(() => <TitleBar onAdd={onAdd} />);
    await fireEvent.click(screen.getByRole("button", { name: "Add game" }));
    const menu = screen.getByRole("menu", { name: "Add game" });
    expect(
      within(menu).getByRole("menuitem", { name: /New Minecraft instance/ }),
    ).toHaveTextContent("Prism Launcher");
    await fireEvent.click(within(menu).getByRole("menuitem", { name: /Add a game/ }));
    expect(onAdd).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("opens an installed launcher", async () => {
    render(() => <TitleBar onAdd={vi.fn<() => void>()} />);
    await fireEvent.click(screen.getByRole("button", { name: "Open launcher" }));
    const menu = screen.getByRole("menu", { name: "Open launcher" });
    expect(
      within(menu)
        .getAllByRole("menuitem")
        .map((i) => i.textContent),
    ).toEqual(["Steam"]);
    await fireEvent.click(within(menu).getByRole("menuitem", { name: "Steam" }));
    expect(api.openLauncher).toHaveBeenCalledWith("steam");
  });

  it("has every app action in the Questkeep menu", async () => {
    render(() => <TitleBar onAdd={vi.fn<() => void>()} />);
    await fireEvent.click(screen.getByRole("button", { name: "Questkeep menu" }));
    const menu = screen.getByRole("menu", { name: "Questkeep" });
    expect(
      within(menu)
        .getAllByRole("menuitem")
        .map((i) => i.textContent),
    ).toEqual([
      "Edit views",
      "Add game",
      "Open launcher",
      "Sync",
      "Games in several stores…",
      "Settings",
      "Exit Questkeep",
    ]);
    await fireEvent.click(within(menu).getByRole("menuitem", { name: "Sync" }));
    const sync = screen.getByRole("menu", { name: "Sync" });
    await fireEvent.click(within(sync).getByRole("menuitem", { name: "Epic Games" }));
    expect(api.scanLibrary).toHaveBeenCalledWith("epic");
    expect(screen.queryByRole("menu")).toBeNull();
  });
});

describe("Command bar", () => {
  it("finds games anywhere and opens the selected one's page", async () => {
    const hades = makeGame({ id: "h", name: "Hades", sortingName: "hades" });
    const halo = makeGame({ id: "o", name: "Halo", sortingName: "halo", hidden: true });
    render(() => <TitleBar onAdd={vi.fn<() => void>()} />);
    const input = screen.getByRole("combobox", { name: "Find a game" });
    vi.mocked(api.listGames).mockResolvedValue([halo, hades]);
    await fireEvent.input(input, { target: { value: "ha" } });
    await vi.waitFor(() => expect(screen.getAllByRole("option")).toHaveLength(2));
    expect(vi.mocked(api.listGames).mock.lastCall?.[0]).toMatchObject({
      search: "ha",
      hidden: "all",
    });
    const options = screen.getAllByRole("option");
    expect(options[0]).toHaveTextContent("Hades");
    expect(options[1]).toHaveTextContent("Hidden");
    await fireEvent.keyDown(input, { key: "ArrowDown" });
    await fireEvent.keyDown(input, { key: "Enter" });
    expect(store.selectedId()).toBe("o");
    expect(store.detailsOpen()).toBe(true);
    expect(screen.queryByRole("option")).toBeNull();
  });

  it("starts apps from the apps launcher and shows all of them in Explore", async () => {
    const blender = makeGame({ id: "b", name: "Blender", kind: "app" });
    vi.mocked(api.listGames).mockResolvedValue([blender]);
    render(() => <TitleBar onAdd={() => undefined} />);
    await fireEvent.click(screen.getByRole("button", { name: "Apps" }));
    const launcher = await screen.findByRole("dialog", { name: "Apps" });
    expect(api.listGames).toHaveBeenCalledWith(
      expect.objectContaining({ kind: "apps", sort: "playtime" }),
    );
    await fireEvent.contextMenu(await within(launcher).findByRole("button", { name: /Blender/ }));
    expect(store.contextMenu()?.game.id).toBe("b");
    // The launcher stays open behind the menu.
    expect(screen.getByRole("dialog", { name: "Apps" })).toBeInTheDocument();
    store.closeContextMenu(false);

    await fireEvent.click(await screen.findByRole("button", { name: "Show all apps" }));
    expect(store.query().kind).toBe("apps");
    expect(screen.queryByRole("dialog", { name: "Apps" })).toBeNull();
  });
});

describe("Signed-out accounts", () => {
  it("show a badge and a dismissable warning in the sync panel", async () => {
    vi.mocked(api.accountWarnings).mockResolvedValue([
      { provider: "steam", name: "Steam", displayName: "Ada" },
    ]);
    vi.mocked(api.dismissAccountWarning).mockResolvedValue(undefined);
    await store.loadAccountWarnings();
    render(() => <TitleBar onAdd={() => undefined} />);
    const sync = screen.getByRole("button", { name: /1 account signed out/ });
    expect(document.querySelector(".titlebar__sync-warn")).not.toBeNull();
    await fireEvent.click(sync);
    const panel = await screen.findByRole("dialog", { name: "Library sync" });
    expect(within(panel).getByText(/Steam \(Ada\) is signed out/)).toBeInTheDocument();
    await fireEvent.click(
      within(panel).getByRole("button", { name: "Dismiss the warning about Steam" }),
    );
    expect(api.dismissAccountWarning).toHaveBeenCalledWith("steam");
    await vi.waitFor(() => expect(document.querySelector(".titlebar__sync-warn")).toBeNull());
  });
});
