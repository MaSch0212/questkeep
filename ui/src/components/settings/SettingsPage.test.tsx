import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
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
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));
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
    providers: {
      epic: { token: "keep-me" },
      steam: { apiKey: "OLDKEY", extraFlag: 7 },
    },
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  store.reset();
  vi.mocked(api.getSettings).mockResolvedValue(makeSettings());
  vi.mocked(api.saveSettings).mockResolvedValue(undefined);
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
    for (const name of ["General", "Library and stores", "Steam account", "About"]) {
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

  it("hides the API key until asked and warns about plain-text storage", async () => {
    render(() => <SettingsPage />);
    const input = await screen.findByLabelText("Steam Web API key");
    await waitFor(() => expect(input).toHaveValue("OLDKEY"));
    expect(input).toHaveAttribute("type", "password");
    await fireEvent.click(screen.getByRole("button", { name: "Show API key" }));
    expect(input).toHaveAttribute("type", "text");
    expect(screen.getByText(/stored unencrypted/)).toBeInTheDocument();
    expect(screen.getByText(/last signed in to Steam/)).toBeInTheDocument();
  });

  it("saves the Steam section without dropping other providers, then rescans", async () => {
    vi.mocked(api.scanLibrary).mockResolvedValue({ added: 3, updated: 1, uninstalled: 0 });
    render(() => <SettingsPage />);
    const key = await screen.findByLabelText("Steam Web API key");
    await waitFor(() => expect(key).toHaveValue("OLDKEY"));
    await fireEvent.input(key, { target: { value: "NEWKEY" } });
    await fireEvent.input(screen.getByLabelText("SteamID64 (optional)"), {
      target: { value: "76561198000000000" },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Save Steam account" }));
    await waitFor(() =>
      expect(api.saveSettings).toHaveBeenCalledWith({
        scanOnStartup: true,
        downloadCovers: true,
        fetchMetadata: true,
        steamGridDbApiKey: null,
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
    const key = await screen.findByLabelText("Steam Web API key");
    await waitFor(() => expect(key).toHaveValue("OLDKEY"));
    await fireEvent.click(screen.getByRole("button", { name: "Save Steam account" }));
    await waitFor(() => expect(api.saveSettings).toHaveBeenCalled());
    expect(api.scanLibrary).not.toHaveBeenCalled();
  });
});
