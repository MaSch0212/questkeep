import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { listen } from "@tauri-apps/api/event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Settings } from "../../bindings/Settings";
import * as api from "../../lib/api";
import { store } from "../../lib/store";
import { withSteamSettings } from "../../lib/steamSettings";
import { SettingsPage } from "./SettingsPage";

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
    downloadCovers: true,
    fetchMetadata: true,
    steamGridDbApiKey: null,
    igdbClientId: null,
    igdbClientSecret: null,
    closeToTray: true,
    minimizeToTray: false,
    startWithSystem: false,
    startMinimized: true,
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

describe("withSteamSettings", () => {
  it("keeps other providers and unknown keys, and drops empty values", () => {
    const next = withSteamSettings(makeSettings(), "  NEW  ", "");
    expect(next.providers).toEqual({
      epic: { token: "keep-me" },
      steam: { apiKey: "NEW", extraFlag: 7 },
    });
    expect(withSteamSettings(makeSettings(), "", "").providers.steam).toEqual({ extraFlag: 7 });
  });
});

describe("SettingsPage", () => {
  it("is organised in headed sections with a section nav", () => {
    render(() => <SettingsPage />);
    const nav = within(screen.getByRole("navigation", { name: "Settings sections" }));
    for (const name of ["General", "Library and stores", "Steam API key", "About"]) {
      expect(screen.getByRole("heading", { name })).toBeInTheDocument();
      expect(nav.getByRole("button", { name })).toBeInTheDocument();
    }
    expect(screen.queryByRole("heading", { name: "Developer" })).toBeNull();
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
    render(() => <SettingsPage />);
    const input = await screen.findByLabelText("Steam Web API key (optional)");
    await waitFor(() => expect(input).toHaveValue("OLDKEY"));
    expect(input).toHaveAttribute("type", "password");
    await fireEvent.click(screen.getByRole("button", { name: "Show API key" }));
    expect(input).toHaveAttribute("type", "text");
    expect(screen.getAllByText(/credential store/).length).toBeGreaterThan(0);
    expect(screen.getByText(/last signed in to Steam/)).toBeInTheDocument();
  });

  it("saves the Steam section without dropping other providers, then rescans", async () => {
    vi.mocked(api.scanLibrary).mockResolvedValue({ added: 3, updated: 1, uninstalled: 0 });
    render(() => <SettingsPage />);
    const key = await screen.findByLabelText("Steam Web API key (optional)");
    await waitFor(() => expect(key).toHaveValue("OLDKEY"));
    await fireEvent.input(key, { target: { value: "NEWKEY" } });
    await fireEvent.input(screen.getByLabelText("SteamID64 (optional)"), {
      target: { value: "76561198000000000" },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Save Steam key" }));
    await waitFor(() =>
      expect(api.saveSettings).toHaveBeenCalledWith({
        scanOnStartup: true,
        downloadCovers: true,
        fetchMetadata: true,
        steamGridDbApiKey: null,
        igdbClientId: null,
        igdbClientSecret: null,
        closeToTray: true,
        minimizeToTray: false,
        startWithSystem: false,
        startMinimized: true,
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
    render(() => <SettingsPage />);
    const key = await screen.findByLabelText("Steam Web API key (optional)");
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
      },
    ]);
    await store.init();
    vi.mocked(api.scanLibrary).mockResolvedValue({
      added: 0,
      updated: 1,
      uninstalled: 0,
    });
    render(() => <SettingsPage />);
    const toggle = await screen.findByRole("checkbox", {
      name: "Launch GOG games through GOG Galaxy",
    });
    expect((toggle as HTMLInputElement).checked).toBe(false);
    fireEvent.change(toggle, { target: { checked: true } });
    await waitFor(() => expect(api.scanLibrary).toHaveBeenCalledWith("gog"));
    const saved = vi.mocked(api.saveSettings).mock.calls.at(-1)?.[0];
    expect(saved?.providers.gog).toEqual({ launchThroughGalaxy: true });
    expect(saved?.providers.steam).toEqual({ apiKey: "OLDKEY", extraFlag: 7 });
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
      },
    ]);
    await store.init();
    vi.mocked(api.scanLibrary).mockResolvedValue({ added: 0, updated: 0, uninstalled: 0 });
    render(() => <SettingsPage />);
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
      },
    ]);
    await store.init();
    vi.mocked(api.scanLibrary).mockResolvedValue({ added: 0, updated: 0, uninstalled: 0 });
    render(() => <SettingsPage />);
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
