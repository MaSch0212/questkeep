import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Settings } from "../../bindings/Settings";
import * as api from "../../lib/api";
import { store } from "../../lib/store";
import { makePluginUpdate } from "../../test-utils";
import { PluginUpdateCheck } from "./PluginUpdateCheck";
import { createSettingsModel } from "./useSettings";

vi.mock("../../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));

function makeSettings(overrides: Partial<Settings> = {}): Settings {
  return {
    scanOnStartup: true,
    checkPluginUpdates: true,
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
    tray: { icon: "questkeep", quickLaunchRows: 3, quickLaunch: null },
    startPage: { background: true, statistics: true, recentlyPlayed: true, recentlyAdded: true },
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
    providers: {},
    ...overrides,
  };
}

function Harness() {
  // Once: a prop expression would run again on every read.
  const model = createSettingsModel();
  return <PluginUpdateCheck model={model} />;
}

const toasts = () => store.toasts().map((t) => t.message);

async function show(settings = makeSettings()) {
  vi.mocked(api.getSettings).mockResolvedValue(settings);
  render(() => <Harness />);
  return screen.findByRole("switch", { name: "Check for plugin updates" });
}

beforeEach(() => {
  vi.clearAllMocks();
  store.reset();
  vi.mocked(api.saveSettings).mockResolvedValue(undefined);
  vi.mocked(api.pluginUpdates).mockResolvedValue([]);
});

describe("PluginUpdateCheck", () => {
  it("shows the setting and saves it when it is toggled", async () => {
    const box = await show();
    expect(box).toBeChecked();
    expect(screen.getByText(/every 12 hours/)).toBeInTheDocument();
    await fireEvent.click(box);
    await waitFor(() =>
      expect(api.saveSettings).toHaveBeenCalledWith(
        expect.objectContaining({ checkPluginUpdates: false }),
      ),
    );
    expect(box).not.toBeChecked();
  });

  it("shows the setting as off", async () => {
    expect(await show(makeSettings({ checkPluginUpdates: false }))).not.toBeChecked();
  });

  it("checks now and says everything is up to date", async () => {
    await show();
    vi.mocked(api.checkPluginUpdatesNow).mockResolvedValue({
      updates: [],
      requested: true,
      failed: false,
    });
    await fireEvent.click(screen.getByRole("button", { name: "Check now" }));
    expect(api.checkPluginUpdatesNow).toHaveBeenCalledTimes(1);
    expect(await screen.findByText("Everything is up to date.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Plugin updates available/ })).toBeNull();
  });

  it("reports the number of updates found, and the Updates button opens the dialog", async () => {
    await show();
    const updates = [makePluginUpdate(), makePluginUpdate({ id: "clock", name: "Clock" })];
    vi.mocked(api.checkPluginUpdatesNow).mockResolvedValue({
      updates,
      requested: true,
      failed: false,
    });
    vi.mocked(api.pluginUpdates).mockResolvedValue(updates);
    await fireEvent.click(screen.getByRole("button", { name: "Check now" }));
    expect(await screen.findByText("2 plugin updates available.")).toBeInTheDocument();
    const open = await screen.findByRole("button", { name: "Plugin updates available: 2" });
    expect(open).toHaveTextContent("Updates");
    expect(store.pluginUpdatesOpen()).toBe(false);
    await fireEvent.click(open);
    expect(store.pluginUpdatesOpen()).toBe(true);
  });

  it("says when there is nothing to check", async () => {
    await show();
    vi.mocked(api.checkPluginUpdatesNow).mockResolvedValue({
      updates: [],
      requested: false,
      failed: false,
    });
    await fireEvent.click(screen.getByRole("button", { name: "Check now" }));
    expect(
      await screen.findByText(
        "No installed plugin comes from the registry, so there is nothing to check.",
      ),
    ).toBeInTheDocument();
  });

  it("shows the reason when the registry can't be reached, and the button works again", async () => {
    await show();
    vi.mocked(api.checkPluginUpdatesNow).mockRejectedValueOnce(
      "The plugin registry could not be loaded: Could not reach example.org.",
    );
    const button = screen.getByRole("button", { name: "Check now" });
    await fireEvent.click(button);
    await waitFor(() =>
      expect(toasts()).toContain(
        "The plugin registry could not be loaded: Could not reach example.org.",
      ),
    );
    expect(screen.getByRole("button", { name: "Check now" })).toBeEnabled();
    expect(screen.queryByRole("status")).toBeNull();
  });
});
