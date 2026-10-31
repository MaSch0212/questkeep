import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { openUrl } from "@tauri-apps/plugin-opener";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Settings } from "../../bindings/Settings";
import * as api from "../../lib/api";
import { store } from "../../lib/store";
import { MetadataSection } from "./MetadataSection";
import { createSettingsModel } from "./useSettings";

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
    steamGridDbApiKey: "sgdb",
    igdbClientId: null,
    igdbClientSecret: null,
    closeToTray: true,
    minimizeToTray: false,
    startWithSystem: false,
    startMinimized: true,
    detectExternalLaunches: true,
    showStartPage: true,
    startupPage: { kind: "start" },
    providers: { epic: { token: "keep-me" } },
    ...overrides,
  };
}

function Harness() {
  const model = createSettingsModel();
  return <MetadataSection model={model} />;
}

const toasts = () => store.toasts().map((t) => t.message);

describe("MetadataSection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    store.reset();
    vi.mocked(api.saveSettings).mockResolvedValue(undefined);
  });

  it("saves trimmed credentials and keeps everything else", async () => {
    vi.mocked(api.getSettings).mockResolvedValue(makeSettings());
    render(() => <Harness />);
    const id = await screen.findByLabelText("IGDB Client ID");
    const secret = screen.getByLabelText("IGDB Client Secret");
    expect(id).toHaveAttribute("type", "text");
    expect(secret).toHaveAttribute("type", "password");
    expect(screen.getByText(/credential store/)).toBeInTheDocument();
    await fireEvent.input(id, { target: { value: "  twitch-id  " } });
    await fireEvent.input(secret, { target: { value: " twitch-secret " } });
    await fireEvent.click(screen.getByRole("button", { name: "Save metadata settings" }));
    await waitFor(() =>
      expect(api.saveSettings).toHaveBeenCalledWith(
        makeSettings({ igdbClientId: "twitch-id", igdbClientSecret: "twitch-secret" }),
      ),
    );
    await waitFor(() => expect(toasts()).toContain("IGDB credentials saved"));
  });

  it("saves empty inputs as null", async () => {
    vi.mocked(api.getSettings).mockResolvedValue(
      makeSettings({ igdbClientId: "old-id", igdbClientSecret: "old-secret" }),
    );
    render(() => <Harness />);
    const id = await screen.findByLabelText("IGDB Client ID");
    const secret = screen.getByLabelText("IGDB Client Secret");
    await waitFor(() => expect(id).toHaveValue("old-id"));
    expect(secret).toHaveValue("old-secret");
    await fireEvent.input(id, { target: { value: "" } });
    await fireEvent.input(secret, { target: { value: "  " } });
    await fireEvent.click(screen.getByRole("button", { name: "Save metadata settings" }));
    await waitFor(() => expect(api.saveSettings).toHaveBeenCalledWith(makeSettings()));
    await waitFor(() => expect(toasts()).toContain("IGDB credentials removed"));
  });

  it("reveals the secret on request and opens the Twitch console", async () => {
    vi.mocked(api.getSettings).mockResolvedValue(makeSettings());
    render(() => <Harness />);
    const secret = await screen.findByLabelText("IGDB Client Secret");
    await fireEvent.click(screen.getByRole("button", { name: "Show IGDB secret" }));
    expect(secret).toHaveAttribute("type", "text");
    expect(screen.getByRole("button", { name: "Hide IGDB secret" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await fireEvent.click(
      screen.getByRole("button", { name: "https://dev.twitch.tv/console/apps" }),
    );
    expect(openUrl).toHaveBeenCalledWith("https://dev.twitch.tv/console/apps");
  });

  it("shows a save failure as a toast and keeps the input", async () => {
    vi.mocked(api.getSettings).mockResolvedValue(makeSettings());
    vi.mocked(api.saveSettings).mockRejectedValue("Could not store the API key");
    render(() => <Harness />);
    const id = await screen.findByLabelText("IGDB Client ID");
    await fireEvent.input(id, { target: { value: "abc" } });
    await fireEvent.click(screen.getByRole("button", { name: "Save metadata settings" }));
    await waitFor(() => expect(toasts()).toContain("Could not store the API key"));
    expect(id).toHaveValue("abc");
    expect(screen.getByRole("button", { name: "Save metadata settings" })).not.toBeDisabled();
  });
});
