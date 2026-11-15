import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { listen } from "@tauri-apps/api/event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Settings } from "../../bindings/Settings";
import * as api from "../../lib/api";
import { store } from "../../lib/store";
import { SettingsPage } from "./SettingsPage";
import { makeProvider, optionSettings, STEAM_SETTINGS } from "../../test-utils";

vi.mock("../../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => Promise<() => void>>() }));
vi.mock("@tauri-apps/plugin-opener", () => ({
  openPath: vi.fn<() => void>(),
  openUrl: vi.fn<() => void>(),
}));

function makeSettings(overrides: Partial<Settings> = {}): Settings {
  return {
    scanOnStartup: true,
    autoSyncMinutes: 15,
    watchInstallFolders: true,
    downloadCovers: true,
    fetchMetadata: true,
    steamGridDbApiKey: null,
    igdbClientId: null,
    igdbClientSecret: null,
    closeToTray: true,
    minimizeToTray: false,
    startWithSystem: false,
    startMinimized: true,
    windowOnGameStart: "nothing",
    restoreWindowAfterGame: false,
    launcherClose: {},
    launcherCloseDelaySeconds: 60,
    launcherCloseMinMinutes: 2,
    systemNotifications: true,
    detectExternalLaunches: true,
    showStartPage: true,
    showStatistics: true,
    startupPage: { kind: "start" },
    sortPrefixes: { enabled: true, words: ["The", "A", "An"] },
    backups: { enabled: true, folder: null, frequency: "weekly", keep: 5 },
    language: null,
    priceSites: [],
    storeOrder: [],
    appearance: { theme: "questkeep", scheme: "system", params: {} },
    groupMerges: {},
    metadataPriority: { order: [], fields: {} },
    providers: {
      epic: { token: "keep-me" },
      steam: { apiKey: "OLDKEY", extraFlag: 7 },
    },
    ...overrides,
  };
}

async function loadAppInfo(dataDir: string, logsDir: string) {
  vi.mocked(listen).mockResolvedValue(() => undefined);
  vi.mocked(api.listProviders).mockResolvedValue([]);
  vi.mocked(api.runningGames).mockResolvedValue([]);
  vi.mocked(api.listGames).mockResolvedValue([]);
  vi.mocked(api.appInfo).mockResolvedValue({
    version: "1",
    dataDir,
    logsDir,
    portable: false,
    devTools: false,
  });
  await store.init();
}

beforeEach(() => {
  vi.clearAllMocks();
  store.reset();
  vi.mocked(api.getSettings).mockResolvedValue(makeSettings());
  vi.mocked(api.saveSettings).mockResolvedValue(undefined);
  vi.mocked(api.accountStatus).mockResolvedValue([]);
});

/** Opens the Steam key form in the Accounts section (a key is saved, so it says "Change"). */
async function openSteamKey() {
  vi.mocked(api.listProviders).mockResolvedValue([
    makeProvider({ id: "steam", name: "Steam", settings: STEAM_SETTINGS }),
  ]);
  await store.loadProviders();
  vi.mocked(api.accountStatus).mockResolvedValue([
    {
      provider: "steam",
      name: "Steam",
      signedIn: false,
      displayName: null,
      supportsCodeSignIn: false,
      codeUrl: null,
      codeHint: null,
    },
  ]);
  render(() => <SettingsPage />);
  expect(await screen.findByText("Using a Steam Web API key")).toBeInTheDocument();
  await fireEvent.click(await screen.findByRole("button", { name: "Steam settings" }));
  await fireEvent.click(screen.getByRole("button", { name: "Change Steam key" }));
}

describe("SettingsPage", () => {
  it("is organised in headed sections with a section nav", () => {
    render(() => <SettingsPage />);
    const nav = within(screen.getByRole("navigation", { name: "Settings sections" }));
    for (const name of ["General", "Stores and accounts", "About"]) {
      expect(screen.getByRole("heading", { name })).toBeInTheDocument();
      expect(nav.getByRole("button", { name })).toBeInTheDocument();
    }
    expect(screen.queryByRole("heading", { name: "Developer" })).toBeNull();
  });

  it("marks a section asked for from elsewhere, also while the page is open", async () => {
    store.showSettingsSection("plugins");
    render(() => <SettingsPage />);
    const nav = within(screen.getByRole("navigation", { name: "Settings sections" }));
    expect(nav.getByRole("button", { name: "Plugins" })).toHaveAttribute(
      "aria-current",
      "location",
    );
    // The link under Plugins that leads to the themes.
    await fireEvent.click(screen.getByRole("button", { name: "Open Appearance" }));
    expect(nav.getByRole("button", { name: "Appearance" })).toHaveAttribute(
      "aria-current",
      "location",
    );
    expect(nav.getByRole("button", { name: "Plugins" })).not.toHaveAttribute("aria-current");
    expect(store.takeSettingsSection()).toBeNull();
  });

  it("toggles store metadata downloads", async () => {
    render(() => <SettingsPage />);
    await fireEvent.click(await screen.findByRole("checkbox", { name: "Download store metadata" }));
    await waitFor(() =>
      expect(api.saveSettings).toHaveBeenCalledWith(
        expect.objectContaining({ fetchMetadata: false }),
      ),
    );
  });

  it("saves the tray toggles", async () => {
    render(() => <SettingsPage />);
    const closeToTray = await screen.findByRole("checkbox", {
      name: "Close to the tray instead of quitting",
    });
    expect(closeToTray).toBeChecked();
    await fireEvent.click(closeToTray);
    await waitFor(() =>
      expect(api.saveSettings).toHaveBeenLastCalledWith(
        expect.objectContaining({ closeToTray: false }),
      ),
    );
    await fireEvent.click(screen.getByRole("checkbox", { name: "Minimize to the tray" }));
    await waitFor(() =>
      expect(api.saveSettings).toHaveBeenLastCalledWith(
        expect.objectContaining({ minimizeToTray: true }),
      ),
    );
  });

  it("turns the start page off and chooses the page shown on start", async () => {
    render(() => <SettingsPage />);
    const show = await screen.findByRole("checkbox", { name: "Show the start page" });
    const startup = screen.getByRole("combobox", { name: "When Questkeep opens, show" });
    expect(startup).toHaveValue("start");
    await fireEvent.click(show);
    await waitFor(() =>
      expect(api.saveSettings).toHaveBeenLastCalledWith(
        expect.objectContaining({ showStartPage: false }),
      ),
    );
    expect(store.showStartPage()).toBe(false);
    expect(screen.getByRole("option", { name: "Start page" })).toBeDisabled();
    expect(screen.getByText(/opens the first view instead/)).toBeInTheDocument();
    await fireEvent.change(startup, { target: { value: "last" } });
    await waitFor(() =>
      expect(api.saveSettings).toHaveBeenLastCalledWith(
        expect.objectContaining({ startupPage: { kind: "last" } }),
      ),
    );
  });

  it("enables start-minimized only while start-with-system is on", async () => {
    render(() => <SettingsPage />);
    const startWithSystem = await screen.findByRole("checkbox", {
      name: "Start Questkeep when I sign in to my computer",
    });
    const minimized = screen.getByRole("checkbox", { name: "Start minimized to the tray" });
    expect(minimized).toBeDisabled();
    await fireEvent.click(startWithSystem);
    await waitFor(() =>
      expect(api.saveSettings).toHaveBeenLastCalledWith(
        expect.objectContaining({ startWithSystem: true }),
      ),
    );
    await waitFor(() => expect(minimized).toBeEnabled());
    await fireEvent.click(minimized);
    await waitFor(() =>
      expect(api.saveSettings).toHaveBeenLastCalledWith(
        expect.objectContaining({ startWithSystem: true, startMinimized: false }),
      ),
    );
  });

  it("reverts a toggle and shows the reason when saving fails", async () => {
    vi.mocked(api.saveSettings).mockRejectedValue("Could not change the start-with-system setting");
    render(() => <SettingsPage />);
    const startWithSystem = await screen.findByRole("checkbox", {
      name: "Start Questkeep when I sign in to my computer",
    });
    await fireEvent.click(startWithSystem);
    await waitFor(() =>
      expect(store.toasts().map((t) => t.message)).toContain(
        "Could not change the start-with-system setting",
      ),
    );
    expect(startWithSystem).not.toBeChecked();
  });

  it("opens the data and logs folders through the app, not the opener plugin", async () => {
    vi.mocked(api.openAppFolder).mockResolvedValue(undefined);
    await loadAppInfo("D:\\data", "D:\\data\\logs");
    render(() => <SettingsPage />);
    const [dataButton, logsButton] = await screen.findAllByRole("button", {
      name: "Open folder",
    });
    await fireEvent.click(dataButton as HTMLElement);
    await fireEvent.click(logsButton as HTMLElement);
    expect(api.openAppFolder).toHaveBeenNthCalledWith(1, "data");
    expect(api.openAppFolder).toHaveBeenNthCalledWith(2, "logs");
  });

  it("shows an error toast when a folder cannot be opened", async () => {
    vi.mocked(api.openAppFolder).mockRejectedValue("Could not open the folder: nope");
    await loadAppInfo("d", "l");
    render(() => <SettingsPage />);
    const [dataButton] = await screen.findAllByRole("button", { name: "Open folder" });
    await fireEvent.click(dataButton as HTMLElement);
    await waitFor(() =>
      expect(store.toasts().map((t) => t.message)).toContain("Could not open the folder: nope"),
    );
  });

  it("hides the API key until asked and explains where it is stored", async () => {
    await openSteamKey();
    const input = screen.getByLabelText("Steam Web API key");
    await waitFor(() => expect(input).toHaveValue("OLDKEY"));
    expect(input).toHaveAttribute("type", "password");
    await fireEvent.click(screen.getByRole("button", { name: "Show Steam Web API key" }));
    expect(input).toHaveAttribute("type", "text");
    expect(screen.getAllByText(/credential store/).length).toBeGreaterThan(0);
    expect(screen.getByText(/last signed in to the Steam client/)).toBeInTheDocument();
  });

  it("saves the Steam section without dropping other providers, then rescans", async () => {
    vi.mocked(api.scanLibrary).mockResolvedValue({ added: 3, updated: 1, uninstalled: 0 });
    await openSteamKey();
    const key = screen.getByLabelText("Steam Web API key");
    await waitFor(() => expect(key).toHaveValue("OLDKEY"));
    await fireEvent.input(key, { target: { value: "NEWKEY" } });
    await fireEvent.input(screen.getByLabelText("SteamID64 (optional)"), {
      target: { value: "76561198000000000" },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Save Steam key" }));
    await waitFor(() =>
      expect(api.saveSettings).toHaveBeenCalledWith({
        scanOnStartup: true,
        autoSyncMinutes: 15,
        watchInstallFolders: true,
        downloadCovers: true,
        fetchMetadata: true,
        steamGridDbApiKey: null,
        igdbClientId: null,
        igdbClientSecret: null,
        closeToTray: true,
        minimizeToTray: false,
        startWithSystem: false,
        startMinimized: true,
        windowOnGameStart: "nothing",
        restoreWindowAfterGame: false,
        launcherClose: {},
        launcherCloseDelaySeconds: 60,
        launcherCloseMinMinutes: 2,
        systemNotifications: true,
        detectExternalLaunches: true,
        showStartPage: true,
        showStatistics: true,
        startupPage: { kind: "start" },
        sortPrefixes: { enabled: true, words: ["The", "A", "An"] },
        backups: { enabled: true, folder: null, frequency: "weekly", keep: 5 },
        language: null,
        priceSites: [],
        storeOrder: [],
        appearance: { theme: "questkeep", scheme: "system", params: {} },
        groupMerges: {},
        metadataPriority: { order: [], fields: {} },
        providers: {
          epic: { token: "keep-me" },
          steam: { apiKey: "NEWKEY", extraFlag: 7, steamId: "76561198000000000" },
        },
      }),
    );
    await waitFor(() => expect(api.scanLibrary).toHaveBeenCalledWith("steam"));
    await waitFor(() =>
      expect(store.toasts().map((t) => t.message)).toContain(
        "Steam: 3 added, 1 updated, 0 uninstalled",
      ),
    );
  });

  it("does not rescan when the credentials did not change", async () => {
    await openSteamKey();
    const key = screen.getByLabelText("Steam Web API key");
    await waitFor(() => expect(key).toHaveValue("OLDKEY"));
    await fireEvent.click(screen.getByRole("button", { name: "Save Steam key" }));
    await waitFor(() => expect(api.saveSettings).toHaveBeenCalled());
    expect(api.scanLibrary).not.toHaveBeenCalled();
  });
});

describe("GOG Galaxy launch option", () => {
  it("saves the option and rescans GOG", async () => {
    await loadAppInfo("C:/data", "C:/data/logs");
    vi.mocked(api.listProviders).mockResolvedValue([
      {
        id: "gog",
        name: "GOG",
        icon: "<svg></svg>",
        canScan: true,
        canScanOwned: false,
        canUninstall: true,
        launcherPages: false,
        launcherClose: null,
        shopUrl: null,
        installed: true,
        syncable: true,
        addGameOptions: [],
        settings: optionSettings("launchThroughGalaxy", "Launch GOG games through GOG Galaxy"),
      },
    ]);
    await store.init();
    vi.mocked(api.scanLibrary).mockResolvedValue({
      added: 0,
      updated: 1,
      uninstalled: 0,
    });
    render(() => <SettingsPage />);
    await fireEvent.click(await screen.findByRole("button", { name: "GOG settings" }));
    const toggle = await screen.findByRole("checkbox", {
      name: "Launch GOG games through GOG Galaxy",
    });
    expect((toggle as HTMLInputElement).checked).toBe(false);
    fireEvent.change(toggle, { target: { checked: true } });
    await waitFor(() => expect(api.scanLibrary).toHaveBeenCalledWith("gog"));
    const saved = vi.mocked(api.saveSettings).mock.calls.at(-1)?.[0];
    expect(saved?.providers.gog).toEqual({ launchThroughGalaxy: true });
    expect(saved?.providers.steam).toEqual({ apiKey: "OLDKEY", extraFlag: 7 });

    // The row also syncs the store, from a small icon button.
    vi.mocked(api.scanLibrary).mockClear();
    const gog = screen.getByRole("group", { name: "GOG" });
    await fireEvent.click(within(gog).getByRole("button", { name: "Sync GOG" }));
    expect(api.scanLibrary).toHaveBeenCalledWith("gog");
    expect(within(gog).getByText("Imports the games installed on this PC")).toBeInTheDocument();
  });

  it("saves the Xbox Game Pass option and rescans Xbox", async () => {
    await loadAppInfo("C:/data", "C:/data/logs");
    vi.mocked(api.listProviders).mockResolvedValue([
      {
        id: "xbox",
        name: "Xbox",
        icon: "<svg></svg>",
        canScan: true,
        canScanOwned: true,
        canUninstall: true,
        launcherPages: false,
        launcherClose: null,
        shopUrl: null,
        installed: true,
        syncable: true,
        addGameOptions: [],
        settings: optionSettings(
          "hideGamePass",
          "Don't import Game Pass games from my Xbox account",
        ),
      },
    ]);
    await store.init();
    vi.mocked(api.scanLibrary).mockResolvedValue({ added: 0, updated: 0, uninstalled: 0 });
    render(() => <SettingsPage />);
    await fireEvent.click(await screen.findByRole("button", { name: "Xbox settings" }));
    const toggle = await screen.findByRole("checkbox", {
      name: "Don't import Game Pass games from my Xbox account",
    });
    expect((toggle as HTMLInputElement).checked).toBe(false);
    fireEvent.change(toggle, { target: { checked: true } });
    await waitFor(() => expect(api.scanLibrary).toHaveBeenCalledWith("xbox"));
    const saved = vi.mocked(api.saveSettings).mock.calls.at(-1)?.[0];
    expect(saved?.providers.xbox).toEqual({ hideGamePass: true });
  });

  it("saves the Ubisoft catalogue option and rescans Ubisoft", async () => {
    await loadAppInfo("C:/data", "C:/data/logs");
    vi.mocked(api.listProviders).mockResolvedValue([
      {
        id: "ubisoft",
        name: "Ubisoft Connect",
        icon: "<svg></svg>",
        canScan: true,
        canScanOwned: true,
        canUninstall: true,
        launcherPages: false,
        launcherClose: null,
        shopUrl: null,
        installed: true,
        syncable: true,
        addGameOptions: [],
        settings: optionSettings(
          "importCatalogue",
          "Import Ubisoft Connect games that are not installed",
        ),
      },
    ]);
    await store.init();
    vi.mocked(api.scanLibrary).mockResolvedValue({ added: 0, updated: 0, uninstalled: 0 });
    render(() => <SettingsPage />);
    await fireEvent.click(await screen.findByRole("button", { name: "Ubisoft Connect settings" }));
    const toggle = await screen.findByRole("checkbox", {
      name: "Import Ubisoft Connect games that are not installed",
    });
    expect((toggle as HTMLInputElement).checked).toBe(false);
    fireEvent.change(toggle, { target: { checked: true } });
    await waitFor(() => expect(api.scanLibrary).toHaveBeenCalledWith("ubisoft"));
    const saved = vi.mocked(api.saveSettings).mock.calls.at(-1)?.[0];
    expect(saved?.providers.ubisoft).toEqual({ importCatalogue: true });
  });

  it("is hidden without the GOG provider", async () => {
    await loadAppInfo("C:/data", "C:/data/logs");
    render(() => <SettingsPage />);
    await screen.findByText("Close to the tray instead of quitting");
    expect(screen.queryByText("Launch GOG games through GOG Galaxy")).toBeNull();
  });
});

const savedSettings = () => vi.mocked(api.saveSettings).mock.calls.at(-1)?.[0];

describe("Launch automation settings", () => {
  it("sets what happens to the window when a game starts", async () => {
    render(() => <SettingsPage />);
    const group = await screen.findByRole("radiogroup", { name: "When a game starts" });
    const restore = screen.getByRole("checkbox", {
      name: "Bring Questkeep back when the game closes",
    });
    // Nothing happens to the window, so there is nothing to bring back.
    expect(within(group).getByRole("radio", { name: "Do nothing" })).toBeChecked();
    expect(restore).toBeDisabled();

    await fireEvent.click(within(group).getByRole("radio", { name: "Hide Questkeep to the tray" }));
    await waitFor(() => expect(savedSettings()?.windowOnGameStart).toBe("hideToTray"));
    expect(restore).toBeEnabled();

    fireEvent.change(restore, { target: { checked: true } });
    await waitFor(() => expect(savedSettings()?.restoreWindowAfterGame).toBe(true));
  });

  it("saves the launcher delays as whole numbers and keeps the old value for bad input", async () => {
    render(() => <SettingsPage />);
    const delay = await screen.findByLabelText("Wait before closing store launchers");
    const minimum = screen.getByLabelText("Don't close launchers after sessions shorter than");
    expect(delay).toHaveValue(60);
    expect(minimum).toHaveValue(2);

    fireEvent.change(delay, { target: { value: "90" } });
    await waitFor(() => expect(savedSettings()?.launcherCloseDelaySeconds).toBe(90));
    fireEvent.change(minimum, { target: { value: "5" } });
    await waitFor(() => expect(savedSettings()?.launcherCloseMinMinutes).toBe(5));

    vi.mocked(api.saveSettings).mockClear();
    fireEvent.change(delay, { target: { value: "-4" } });
    expect(api.saveSettings).not.toHaveBeenCalled();
    expect(delay).toHaveValue(90);
  });

  it("turns system notifications off", async () => {
    render(() => <SettingsPage />);
    const box = await screen.findByRole("checkbox", {
      name: "Show system notifications while Questkeep is in the background",
    });
    expect(box).toBeChecked();
    fireEvent.change(box, { target: { checked: false } });
    await waitFor(() => expect(savedSettings()?.systemNotifications).toBe(false));
  });

  it("offers closing the launcher only for stores that have one", async () => {
    vi.mocked(api.listProviders).mockResolvedValue([
      makeProvider({
        id: "steam",
        name: "Steam",
        launcherClose: {
          command: {
            kind: "file",
            target: "steam.exe",
            args: "-shutdown",
            workingDir: null,
            tracking: "default",
            trackingPath: null,
          },
        },
      }),
      makeProvider({ id: "epic", name: "Epic Games", launcherClose: "forceQuit" }),
      makeProvider({ id: "prism", name: "Prism Launcher" }),
    ]);
    await store.loadProviders();
    render(() => <SettingsPage />);

    const steam = await screen.findByRole("group", { name: "Steam" });
    await fireEvent.click(within(steam).getByRole("button", { name: "Steam settings" }));
    const choice = within(steam).getByRole("radiogroup", { name: "Close Steam after playing" });
    expect(within(choice).getByRole("radio", { name: "Never" })).toBeChecked();
    expect(
      within(steam).getByText(/asked to shut down cleanly 60 s after the game closes/),
    ).toBeInTheDocument();

    await fireEvent.click(
      within(choice).getByRole("radio", {
        name: "Only if it wasn't running when the game started",
      }),
    );
    await waitFor(() =>
      expect(savedSettings()?.launcherClose).toEqual({ steam: "ifStartedByGame" }),
    );
    expect(within(choice).getByRole("radio", { name: /Only if it wasn't/ })).toBeChecked();

    const epic = screen.getByRole("group", { name: "Epic Games" });
    await fireEvent.click(within(epic).getByRole("button", { name: "Epic Games settings" }));
    expect(within(epic).getByText(/closed by force/)).toBeInTheDocument();
    await fireEvent.click(
      within(
        within(epic).getByRole("radiogroup", { name: "Close Epic Games after playing" }),
      ).getByRole("radio", { name: "Always" }),
    );
    await waitFor(() =>
      expect(savedSettings()?.launcherClose).toEqual({ steam: "ifStartedByGame", epic: "always" }),
    );

    // Choosing Never again removes the entry.
    await fireEvent.click(within(choice).getByRole("radio", { name: "Never" }));
    await waitFor(() => expect(savedSettings()?.launcherClose).toEqual({ epic: "always" }));

    // Prism has no launcher to close: no row to expand.
    const prism = screen.queryByRole("group", { name: "Prism Launcher" });
    expect(prism ? within(prism).queryByRole("radiogroup") : null).toBeNull();
  });
});
