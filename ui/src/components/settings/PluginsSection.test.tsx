import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { open } from "@tauri-apps/plugin-dialog";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { PluginInfo } from "../../bindings/PluginInfo";
import type { PluginPreview } from "../../bindings/PluginPreview";
import * as api from "../../lib/api";
import { resetPluginThemes, setThemeFetcher, syncPluginThemes } from "../../lib/pluginThemes";
import { store } from "../../lib/store";
import { makePlugin } from "../../test-utils";
import { PluginsSection } from "./PluginsSection";

vi.mock("../../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn<() => void>() }));

const makePreview = (overrides: Partial<PluginPreview> = {}): PluginPreview => ({
  id: "new",
  name: "New Store",
  version: "1.2.0",
  description: "Games from the New Store.",
  author: "Someone",
  stores: 1,
  hasExtension: false,
  themes: [],
  permissions: { read: [], http: [] },
  installedVersion: null,
  ...overrides,
});

const toasts = () => store.toasts().map((t) => t.message);

async function show(...plugins: PluginInfo[]) {
  vi.mocked(api.listPlugins).mockResolvedValue(plugins);
  await store.loadPlugins();
  render(() => <PluginsSection />);
}

const row = (name: string) => within(screen.getByRole("group", { name }));

beforeEach(() => {
  vi.clearAllMocks();
  store.reset();
  vi.mocked(api.scanLibrary).mockResolvedValue({ added: 2, updated: 0, uninstalled: 0 });
  vi.mocked(api.previewPlugin).mockResolvedValue(makePreview());
});

describe("PluginsSection", () => {
  it("says so when no plugin is installed", async () => {
    await show();
    expect(screen.getByText("No plugins installed.")).toBeInTheDocument();
  });

  it("lists plugins by name with their kind and status", async () => {
    await show(
      makePlugin({ id: "b", name: "Beta", author: "Ada", description: "Beta games" }),
      makePlugin({ id: "a", name: "Alpha", enabled: false, loaded: false, themes: 1 }),
    );
    const names = screen.getAllByRole("group").map((g) => g.getAttribute("aria-label"));
    expect(names).toEqual(["Alpha", "Beta"]);
    expect(row("Beta").getByText("by Ada")).toBeInTheDocument();
    expect(row("Beta").getByText("Beta games")).toBeInTheDocument();
    expect(row("Beta").getByText("Store")).toBeInTheDocument();
    expect(row("Beta").getByText("Loaded")).toBeInTheDocument();
    expect(row("Alpha").getByText("Theme")).toBeInTheDocument();
    expect(row("Alpha").getByText("Disabled")).toBeInTheDocument();
  });

  it("shows why a plugin failed to load", async () => {
    await show(makePlugin({ loaded: false, error: "plugin.wasm is not a component" }));
    expect(screen.getByText("Failed to load")).toBeInTheDocument();
    expect(screen.getByText("plugin.wasm is not a component")).toHaveClass("plugin__error");
  });

  it("shows source, stores and permissions in the details", async () => {
    await show(
      makePlugin({
        source: "C:\\Dev\\x",
        sourceExists: false,
        permissions: { read: ["%LOCALAPPDATA%/X"], http: ["api.x.com"] },
      }),
    );
    const details = screen.getByRole("button", { name: "Details of Example Store" });
    expect(details).toHaveAttribute("aria-expanded", "false");
    await fireEvent.click(details);
    expect(details).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText(/C:\\Dev\\x/)).toBeInTheDocument();
    expect(screen.getByText(/source folder is gone/)).toBeInTheDocument();
    expect(screen.getByText("example-store")).toBeInTheDocument();
    expect(screen.getByText("%LOCALAPPDATA%/X")).toBeInTheDocument();
    expect(screen.getByText("api.x.com")).toBeInTheDocument();
  });

  it("says when a plugin needs no extra permissions", async () => {
    await show(makePlugin());
    await fireEvent.click(screen.getByRole("button", { name: "Details of Example Store" }));
    expect(screen.getByText("No extra permissions")).toBeInTheDocument();
  });

  it("asks before installing, then installs and scans the plugin's stores", async () => {
    await show();
    vi.mocked(open).mockResolvedValue("C:\\Dev\\new-plugin");
    vi.mocked(api.installPlugin).mockResolvedValue(
      makePlugin({ id: "new", name: "New Store", providers: ["new-a", "new-b"] }),
    );
    await fireEvent.click(screen.getByRole("button", { name: "Install from folder…" }));
    expect(open).toHaveBeenCalledWith(expect.objectContaining({ directory: true }));
    expect(await screen.findByText(/Plugins are third-party code/)).toBeInTheDocument();
    expect(api.installPlugin).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole("button", { name: "Install" }));
    await waitFor(() => expect(api.installPlugin).toHaveBeenCalledWith("C:\\Dev\\new-plugin"));
    await waitFor(() => expect(api.scanLibrary).toHaveBeenCalledTimes(2));
    expect(api.scanLibrary).toHaveBeenCalledWith("new-a");
    expect(api.scanLibrary).toHaveBeenCalledWith("new-b");
    expect(toasts()).toContain("Installed New Store.");
  });

  it("shows what the plugin is before it is installed", async () => {
    await show();
    vi.mocked(open).mockResolvedValue("C:/Dev/new-plugin");
    vi.mocked(api.previewPlugin).mockResolvedValue(
      makePreview({
        hasExtension: true,
        themes: ["Arcade", "Neon"],
        permissions: { read: ["%APPDATA%/NewStore"], http: ["api.new.example"] },
        installedVersion: "1.0.0",
      }),
    );
    await fireEvent.click(screen.getByRole("button", { name: "Install from folder…" }));
    const dialog = await screen.findByRole("dialog");
    expect(api.previewPlugin).toHaveBeenCalledWith("C:/Dev/new-plugin");
    const shown = within(dialog);
    expect(shown.getByText("New Store")).toBeInTheDocument();
    expect(shown.getByText("1.2.0")).toBeInTheDocument();
    expect(shown.getByText("by Someone")).toBeInTheDocument();
    expect(shown.getByText("Games from the New Store.")).toBeInTheDocument();
    for (const kind of ["Store", "Extension", "Theme"]) {
      expect(shown.getByText(kind)).toBeInTheDocument();
    }
    expect(shown.getByText("Arcade, Neon")).toBeInTheDocument();
    expect(shown.getByText("%APPDATA%/NewStore")).toBeInTheDocument();
    expect(shown.getByText("api.new.example")).toBeInTheDocument();
    expect(shown.getByText("Replaces the installed version 1.0.0.")).toBeInTheDocument();
    expect(shown.getByText("C:/Dev/new-plugin")).toBeInTheDocument();
    // Installing is not a destructive action: no danger button, and Cancel has the focus.
    expect(shown.getByRole("button", { name: "Install" })).toHaveClass("btn--primary");
    expect(shown.getByRole("button", { name: "Cancel" })).toHaveFocus();
  });

  it("says why a chosen folder is no plugin, without asking to install", async () => {
    await show();
    vi.mocked(open).mockResolvedValue("C:/Dev/not-a-plugin");
    vi.mocked(api.previewPlugin).mockRejectedValue("plugin.json is missing");
    await fireEvent.click(screen.getByRole("button", { name: "Install from folder…" }));
    await waitFor(() => expect(toasts()).toContain("plugin.json is missing"));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(api.installPlugin).not.toHaveBeenCalled();
  });

  it("filters the file dialog to zip files and installs nothing when cancelled", async () => {
    await show();
    vi.mocked(open).mockResolvedValue(null);
    await fireEvent.click(screen.getByRole("button", { name: "Install from file…" }));
    await waitFor(() =>
      expect(open).toHaveBeenCalledWith(
        expect.objectContaining({
          filters: [{ name: "Questkeep plugin", extensions: ["zip"] }],
        }),
      ),
    );
    expect(screen.queryByText(/Plugins are third-party code/)).toBeNull();
    expect(api.installPlugin).not.toHaveBeenCalled();
  });

  it("does not install when the confirmation is cancelled", async () => {
    await show();
    vi.mocked(open).mockResolvedValue("C:\\Dev\\p.zip");
    await fireEvent.click(screen.getByRole("button", { name: "Install from file…" }));
    await screen.findByText(/Plugins are third-party code/);
    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(api.installPlugin).not.toHaveBeenCalled();
  });

  it("reports a failed install in a toast", async () => {
    await show();
    vi.mocked(open).mockResolvedValue("C:\\Dev\\bad");
    vi.mocked(api.installPlugin).mockRejectedValue("plugin.json is missing");
    await fireEvent.click(screen.getByRole("button", { name: "Install from folder…" }));
    await fireEvent.click(await screen.findByRole("button", { name: "Install" }));
    await waitFor(() => expect(toasts()).toContain("plugin.json is missing"));
    expect(api.scanLibrary).not.toHaveBeenCalled();
  });

  it("reloads a plugin and scans its stores", async () => {
    await show(makePlugin());
    vi.mocked(api.reloadPlugin).mockResolvedValue(makePlugin());
    await fireEvent.click(row("Example Store").getByRole("button", { name: "Reload" }));
    await waitFor(() => expect(api.reloadPlugin).toHaveBeenCalledWith("example-store"));
    await waitFor(() => expect(api.scanLibrary).toHaveBeenCalledWith("example-store"));
    expect(toasts()).toContain("Reloaded Example Store.");
  });

  it("cannot reload a disabled plugin", async () => {
    await show(makePlugin({ enabled: false, loaded: false }));
    expect(row("Example Store").getByRole("button", { name: "Reload" })).toBeDisabled();
    expect(row("Example Store").getByRole("button", { name: "Enable" })).toBeEnabled();
  });

  it("keeps a failure in the row until the next success", async () => {
    await show(makePlugin());
    vi.mocked(api.reloadPlugin).mockRejectedValueOnce("copy failed: access denied");
    await fireEvent.click(row("Example Store").getByRole("button", { name: "Reload" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("copy failed: access denied");
    expect(toasts()).toContain("copy failed: access denied");
    vi.mocked(api.reloadPlugin).mockResolvedValueOnce(makePlugin());
    await fireEvent.click(row("Example Store").getByRole("button", { name: "Reload" }));
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  });

  it("disables the row's buttons while an action runs", async () => {
    await show(makePlugin());
    const holder: { finish: (info: PluginInfo) => void } = { finish: () => undefined };
    vi.mocked(api.setPluginEnabled).mockReturnValue(
      new Promise<PluginInfo>((resolve) => (holder.finish = resolve)),
    );
    await fireEvent.click(row("Example Store").getByRole("button", { name: "Disable" }));
    expect(row("Example Store").getByRole("button", { name: "Reload" })).toBeDisabled();
    expect(row("Example Store").getByRole("button", { name: "Disable" })).toBeDisabled();
    holder.finish(makePlugin({ enabled: false, loaded: false }));
    await waitFor(() =>
      expect(row("Example Store").getByRole("button", { name: "Disable" })).toBeEnabled(),
    );
    expect(api.scanLibrary).not.toHaveBeenCalled();
  });

  it("scans the stores after enabling a plugin", async () => {
    await show(makePlugin({ enabled: false, loaded: false }));
    vi.mocked(api.setPluginEnabled).mockResolvedValue(makePlugin());
    await fireEvent.click(row("Example Store").getByRole("button", { name: "Enable" }));
    await waitFor(() => expect(api.setPluginEnabled).toHaveBeenCalledWith("example-store", true));
    await waitFor(() => expect(api.scanLibrary).toHaveBeenCalledWith("example-store"));
  });

  it("toggles reloading on change, unless the source is gone", async () => {
    await show(makePlugin(), makePlugin({ id: "gone", name: "Gone", sourceExists: false }));
    vi.mocked(api.setPluginWatch).mockResolvedValue(makePlugin({ watch: true }));
    const watch = row("Example Store").getByRole("checkbox", { name: "Reload on change" });
    await fireEvent.click(watch);
    await waitFor(() => expect(api.setPluginWatch).toHaveBeenCalledWith("example-store", true));
    expect(row("Gone").getByRole("checkbox", { name: "Reload on change" })).toBeDisabled();
  });

  it("removes a plugin, optionally with its data", async () => {
    await show(makePlugin());
    vi.mocked(api.removePlugin).mockResolvedValue(undefined);
    await fireEvent.click(row("Example Store").getByRole("button", { name: "Remove…" }));
    await fireEvent.click(
      screen.getByRole("checkbox", { name: "Also delete the plugin's own data" }),
    );
    await fireEvent.click(screen.getByRole("button", { name: "Remove plugin" }));
    await waitFor(() => expect(api.removePlugin).toHaveBeenCalledWith("example-store", true));
    expect(toasts()).toContain("Removed Example Store.");
  });

  it("says where the themes of plugins are and why one could not be loaded", async () => {
    setThemeFetcher(() => Promise.reject(new Error("HTTP 404")));
    const themed = makePlugin({
      id: "neon",
      name: "Neon Pack",
      providers: [],
      themes: 2,
      themeDefs: [
        { id: "neon", name: "Neon", css: "theme.css" },
        { id: "other", name: "Other", css: "theme.css" },
      ],
    });
    await syncPluginThemes([themed]);
    await show(themed, makePlugin());
    expect(
      row("Neon Pack")
        .getAllByRole("alert")
        .map((line) => line.textContent),
    ).toEqual([
      "Theme “Neon” could not be loaded: theme.css: HTTP 404",
      "Theme “Other” could not be loaded: its id “other” must be “neon” or start with “neon-”",
    ]);
    expect(row("Example Store").queryByRole("alert")).toBeNull();

    expect(
      screen.getByText(/Themes that plugins bring are chosen under Appearance./),
    ).toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: "Open Appearance" }));
    expect(store.page()).toBe("settings");
    expect(store.takeSettingsSection()).toBe("appearance");
    setThemeFetcher(null);
    resetPluginThemes();
  });

  it("keeps the plugin's data by default", async () => {
    await show(makePlugin());
    vi.mocked(api.removePlugin).mockResolvedValue(undefined);
    await fireEvent.click(row("Example Store").getByRole("button", { name: "Remove…" }));
    await fireEvent.click(screen.getByRole("button", { name: "Remove plugin" }));
    await waitFor(() => expect(api.removePlugin).toHaveBeenCalledWith("example-store", false));
  });
});
