import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Settings } from "../bindings/Settings";
import * as api from "../lib/api";
import { store } from "../lib/store";
import { FirstRunDialog } from "./FirstRunDialog";
import { IgdbHint } from "./IgdbHint";

vi.mock("../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));
vi.mock("@tauri-apps/plugin-opener", () => ({
  openPath: vi.fn<() => void>(),
  openUrl: vi.fn<() => void>(),
}));

const SETTINGS: Settings = {
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
  detectExternalLaunches: true,
  showStartPage: true,
  startupPage: { kind: "start" },
  sortPrefixes: { enabled: true, words: ["The", "A", "An"] },
  backups: { enabled: true, folder: null, frequency: "weekly", keep: 5 },
  language: null,
  priceSites: [],
  storeOrder: [],
  metadataPriority: { order: [], fields: {} },
  providers: {},
};

const click = (name: string) => fireEvent.click(screen.getByRole("button", { name }));

/** Opens the dialog and walks to the keys step. */
async function toKeys() {
  render(() => <FirstRunDialog />);
  await click("Get started");
  await click("Next");
  await screen.findByLabelText("IGDB Client ID");
}

describe("FirstRunDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    store.reset();
    vi.mocked(api.getSettings).mockResolvedValue(SETTINGS);
    vi.mocked(api.accountStatus).mockResolvedValue([]);
    vi.mocked(api.saveSettings).mockResolvedValue(undefined);
    vi.mocked(api.finishSetup).mockResolvedValue(undefined);
    vi.mocked(api.setupState).mockResolvedValue({ showSetup: false, showIgdbHint: false });
  });

  it("counts as done as soon as it shows, so closing the app skips it", () => {
    render(() => <FirstRunDialog />);
    expect(screen.getByRole("heading", { name: "Welcome to Questkeep" })).toBeInTheDocument();
    expect(api.finishSetup).toHaveBeenCalledTimes(1);
  });

  it("saves both keys and ends the setup on Finish", async () => {
    await toKeys();
    await fireEvent.input(screen.getByLabelText("IGDB Client ID"), { target: { value: " id " } });
    await fireEvent.input(screen.getByLabelText("IGDB Client Secret"), {
      target: { value: "secret" },
    });
    await fireEvent.input(screen.getByLabelText("SteamGridDB API key"), {
      target: { value: "grid" },
    });
    await click("Finish");
    await waitFor(() =>
      expect(api.saveSettings).toHaveBeenCalledWith({
        ...SETTINGS,
        igdbClientId: "id",
        igdbClientSecret: "secret",
        steamGridDbApiKey: "grid",
      }),
    );
    await waitFor(() => expect(api.setupState).toHaveBeenCalled());
    expect(api.finishSetup).toHaveBeenCalledTimes(2);
  });

  it("warns before finishing without IGDB and keeps the other key", async () => {
    await toKeys();
    await fireEvent.input(screen.getByLabelText("SteamGridDB API key"), {
      target: { value: "grid" },
    });
    await click("Finish");
    expect(screen.getByRole("heading", { name: "Continue without IGDB?" })).toBeInTheDocument();
    expect(screen.getByText(/Time to beat/)).toBeInTheDocument();
    await click("Back to keys");
    expect(screen.getByLabelText("SteamGridDB API key")).toHaveValue("grid");
    await click("Finish");
    await click("Continue without");
    await waitFor(() =>
      expect(api.saveSettings).toHaveBeenCalledWith({ ...SETTINGS, steamGridDbApiKey: "grid" }),
    );
  });

  it("skipping saves nothing, after the same warning", async () => {
    render(() => <FirstRunDialog />);
    await click("Skip setup");
    expect(screen.getByRole("heading", { name: "Continue without IGDB?" })).toBeInTheDocument();
    await click("Continue without");
    await waitFor(() => expect(api.setupState).toHaveBeenCalled());
    expect(api.saveSettings).not.toHaveBeenCalled();
  });

  it("Escape skips and Enter presses the primary button", async () => {
    render(() => <FirstRunDialog />);
    // Keys typed on the dialog content (not on a button) bubble up to it.
    const dialog = screen.getByRole("heading", { name: "Welcome to Questkeep" })
      .parentElement as HTMLElement;
    await fireEvent.keyDown(dialog, { key: "Enter" });
    expect(screen.getByRole("heading", { name: "Your stores" })).toBeInTheDocument();
    await fireEvent.keyDown(dialog, { key: "Escape" });
    expect(screen.getByRole("heading", { name: "Continue without IGDB?" })).toBeInTheDocument();
    // On the warning, Escape goes back instead of ending the setup.
    await fireEvent.keyDown(dialog, { key: "Escape" });
    expect(screen.getByLabelText("IGDB Client ID")).toBeInTheDocument();
  });

  it("tests the typed keys and shows the result inline", async () => {
    vi.mocked(api.testIgdbCredentials)
      .mockResolvedValueOnce(undefined)
      .mockRejectedValueOnce("Twitch did not accept this Client ID and Secret.");
    await toKeys();
    const test = screen.getByRole("button", { name: "Test keys" });
    expect(test).toBeDisabled();
    await fireEvent.input(screen.getByLabelText("IGDB Client ID"), { target: { value: "id" } });
    await fireEvent.input(screen.getByLabelText("IGDB Client Secret"), {
      target: { value: "secret" },
    });
    await fireEvent.click(test);
    expect(await screen.findByText("✓ The keys work.")).toBeInTheDocument();
    expect(api.testIgdbCredentials).toHaveBeenCalledWith("id", "secret");
    await fireEvent.input(screen.getByLabelText("IGDB Client Secret"), {
      target: { value: "wrong" },
    });
    expect(screen.queryByText("✓ The keys work.")).not.toBeInTheDocument();
    await fireEvent.click(test);
    expect(await screen.findByText(/did not accept/)).toBeInTheDocument();
    expect(api.saveSettings).not.toHaveBeenCalled();
  });
});

describe("IgdbHint", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    store.reset();
  });

  it("opens Settings → Metadata or goes away for good", async () => {
    vi.mocked(api.dismissIgdbHint).mockResolvedValue(undefined);
    render(() => <IgdbHint />);
    expect(screen.getByText(/no IGDB key is set/)).toBeInTheDocument();
    await click("Set up");
    expect(store.page()).toBe("settings");
    expect(store.takeSettingsSection()).toBe("metadata");
    await click("Don't show again");
    expect(api.dismissIgdbHint).toHaveBeenCalled();
  });
});
