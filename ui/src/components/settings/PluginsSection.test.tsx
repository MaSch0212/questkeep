import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { open } from "@tauri-apps/plugin-dialog";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { PluginFolder } from "../../bindings/PluginFolder";
import type { PluginInfo } from "../../bindings/PluginInfo";
import type { PluginPreview } from "../../bindings/PluginPreview";
import * as api from "../../lib/api";
import { resetPluginThemes, setThemeFetcher, syncPluginThemes } from "../../lib/pluginThemes";
import { store } from "../../lib/store";
import { makePlugin, makePluginUpdate, NO_CHANGES } from "../../test-utils";
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
  icon: null,
  stores: 1,
  hasExtension: false,
  themes: [],
  permissions: { read: [], http: [], folders: [] },
  folders: [],
  installedVersion: null,
  ...overrides,
});

const folder = (overrides: Partial<PluginFolder> = {}): PluginFolder => ({
  key: "instances",
  label: "BSManager instances folder",
  hint: "The folder that contains your Beat Saber instances.",
  mount: "/instances",
  default: "~/BSManager/BSInstances",
  defaultPath: null,
  defaultExists: false,
  path: null,
  effectivePath: null,
  exists: false,
  ...overrides,
});
const withFolders = (...folders: PluginFolder[]) =>
  makePlugin({
    id: "bs",
    name: "Beat Saber",
    folders,
    permissions: {
      read: [],
      http: [],
      folders: folders.map(({ key, label, hint, default: d }) => ({
        key,
        label,
        hint,
        default: d,
      })),
    },
  });
const chooseButton = () =>
  screen.getByRole("button", { name: "Choose the folder for BSManager instances folder" });

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
  vi.mocked(api.pluginFileUrl).mockImplementation((id, path) => `qk://plugins/${id}/${path}`);
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

  it("shows a plugin's icon in its row, as an image from the plugin's files", async () => {
    await show(
      makePlugin({ id: "pic", name: "Pic", icon: "img/logo.svg", revision: 3 }),
      makePlugin({ id: "plain", name: "Plain" }),
    );
    const img = screen.getByRole("group", { name: "Pic" }).querySelector("img");
    expect(img).toHaveAttribute("src", "qk://plugins/pic/@3/img/logo.svg");
    expect(api.pluginFileUrl).toHaveBeenCalledWith("pic", "@3/img/logo.svg");
    // Without an icon the generic puzzle piece stays, and nothing is requested.
    const plain = screen.getByRole("group", { name: "Plain" });
    expect(plain.querySelector(".plugin-icon img")).toBeNull();
    expect(plain.querySelector(".plugin-icon svg")).not.toBeNull();
    expect(api.pluginFileUrl).not.toHaveBeenCalledWith("plain", expect.anything());
  });

  it("falls back to the generic icon when the image does not load", async () => {
    await show(makePlugin({ id: "pic", name: "Pic", icon: "logo.png" }));
    const group = screen.getByRole("group", { name: "Pic" });
    const img = group.querySelector("img");
    expect(img).not.toBeNull();
    await fireEvent.error(img as HTMLImageElement);
    expect(group.querySelector(".plugin-icon img")).toBeNull();
    expect(group.querySelector(".plugin-icon svg")).not.toBeNull();
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
        permissions: { read: ["%LOCALAPPDATA%/X"], http: ["api.x.com"], folders: [] },
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

  it("shows the icon of the plugin in the install dialog, when it has one", async () => {
    await show();
    vi.mocked(open).mockResolvedValue("C:/Dev/new-plugin");
    vi.mocked(api.previewPlugin).mockResolvedValueOnce(
      makePreview({ icon: "data:image/svg+xml;base64,PHN2Zy8+" }),
    );
    await fireEvent.click(screen.getByRole("button", { name: "Install from folder…" }));
    const dialog = await screen.findByRole("dialog");
    expect(dialog.querySelector(".plugin-icon img")).toHaveAttribute(
      "src",
      "data:image/svg+xml;base64,PHN2Zy8+",
    );
    await fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));

    await fireEvent.click(screen.getByRole("button", { name: "Install from folder…" }));
    const plain = await screen.findByRole("dialog");
    expect(plain.querySelector(".plugin-icon img")).toBeNull();
    expect(plain.querySelector(".plugin-icon svg")).not.toBeNull();
  });

  it("shows what the plugin is before it is installed", async () => {
    await show();
    vi.mocked(open).mockResolvedValue("C:/Dev/new-plugin");
    vi.mocked(api.previewPlugin).mockResolvedValue(
      makePreview({
        hasExtension: true,
        themes: ["Arcade", "Neon"],
        permissions: {
          read: ["%APPDATA%/NewStore"],
          http: ["api.new.example"],
          folders: [],
        },
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

  describe("folders the plugin asks for", () => {
    it("shows what each folder is for and where it points", async () => {
      await show(
        withFolders(
          folder({ effectivePath: "C:\\Users\\me\\BSManager\\BSInstances", exists: true }),
          folder({ key: "mods", label: "Mods", hint: null, default: null }),
          folder({
            key: "saves",
            label: "Saves",
            hint: null,
            path: "D:\\Saves",
            effectivePath: "D:\\Saves",
            exists: false,
          }),
        ),
      );
      const instances = within(screen.getByRole("group", { name: "BSManager instances folder" }));
      expect(
        instances.getByText("The folder that contains your Beat Saber instances."),
      ).toBeVisible();
      expect(instances.getByText("Default: C:\\Users\\me\\BSManager\\BSInstances")).toBeVisible();
      expect(instances.queryByRole("alert")).toBeNull();
      // No choice yet: nothing to go back to.
      expect(instances.queryByRole("button", { name: /Use the default folder/ })).toBeNull();
      expect(
        within(screen.getByRole("group", { name: "Mods" })).getByText("Not set"),
      ).toBeVisible();
      const saves = within(screen.getByRole("group", { name: "Saves" }));
      expect(saves.getByText("D:\\Saves")).toBeVisible();
      expect(saves.getByRole("alert")).toHaveTextContent("This folder was not found");
      expect(saves.getByRole("button", { name: "Use the default folder for Saves" })).toBeEnabled();
    });

    it("shows no folder block for a plugin that asks for none", async () => {
      await show(makePlugin());
      expect(screen.queryByText("Folders")).toBeNull();
    });

    it("chooses a folder, reloads the plugin's stores and scans them", async () => {
      await show(withFolders(folder()));
      vi.mocked(open).mockResolvedValue("D:\\BSInstances");
      vi.mocked(api.setPluginFolder).mockResolvedValue(
        makePlugin({
          id: "bs",
          name: "Beat Saber",
          providers: ["bs-store"],
          folders: [
            folder({ path: "D:\\BSInstances", effectivePath: "D:\\BSInstances", exists: true }),
          ],
        }),
      );
      await fireEvent.click(chooseButton());
      expect(open).toHaveBeenCalledWith(
        expect.objectContaining({
          directory: true,
          title: "Choose the folder: BSManager instances folder",
        }),
      );
      await waitFor(() =>
        expect(api.setPluginFolder).toHaveBeenCalledWith("bs", "instances", "D:\\BSInstances"),
      );
      await waitFor(() => expect(api.scanLibrary).toHaveBeenCalledWith("bs-store"));
      expect(toasts()).toContain("Beat Saber: BSManager instances folder is now D:\\BSInstances.");
    });

    it("starts the dialog in the folder that is set, if it exists", async () => {
      await show(withFolders(folder({ effectivePath: "C:\\BS", exists: true })));
      vi.mocked(open).mockResolvedValue(null);
      await fireEvent.click(chooseButton());
      expect(open).toHaveBeenCalledWith(expect.objectContaining({ defaultPath: "C:\\BS" }));
    });

    it("changes nothing when the folder dialog is cancelled", async () => {
      await show(withFolders(folder()));
      vi.mocked(open).mockResolvedValue(null);
      await fireEvent.click(chooseButton());
      await waitFor(() => expect(open).toHaveBeenCalled());
      expect(api.setPluginFolder).not.toHaveBeenCalled();
      expect(api.scanLibrary).not.toHaveBeenCalled();
    });

    it("shows why a folder was refused and scans nothing", async () => {
      await show(withFolders(folder()));
      vi.mocked(open).mockResolvedValue("C:\\Data");
      vi.mocked(api.setPluginFolder).mockRejectedValue(
        "That folder is Questkeep's own data folder, or contains it. Choose another folder.",
      );
      await fireEvent.click(chooseButton());
      expect(await screen.findByRole("alert")).toHaveTextContent("own data folder");
      expect(api.scanLibrary).not.toHaveBeenCalled();
    });

    it("goes back to the default folder", async () => {
      await show(withFolders(folder({ path: "D:\\BS", effectivePath: "D:\\BS", exists: true })));
      vi.mocked(api.setPluginFolder).mockResolvedValue(
        makePlugin({ id: "bs", name: "Beat Saber", providers: ["bs-store"], folders: [folder()] }),
      );
      await fireEvent.click(
        screen.getByRole("button", {
          name: "Use the default folder for BSManager instances folder",
        }),
      );
      await waitFor(() =>
        expect(api.setPluginFolder).toHaveBeenCalledWith("bs", "instances", null),
      );
      await waitFor(() => expect(api.scanLibrary).toHaveBeenCalledWith("bs-store"));
      expect(toasts()).toContain("Beat Saber: BSManager instances folder is back to its default.");
    });

    it("lists the folders in the install dialog", async () => {
      await show();
      vi.mocked(open).mockResolvedValue("C:/Dev/bs");
      vi.mocked(api.previewPlugin).mockResolvedValue(
        makePreview({
          permissions: {
            read: [],
            http: [],
            folders: [
              {
                key: "instances",
                label: "BSManager instances folder",
                hint: "Where your instances are.",
                default: null,
              },
            ],
          },
          folders: [
            {
              key: "instances",
              label: "BSManager instances folder",
              hint: "Where your instances are.",
              mount: "/instances",
              default: null,
              defaultPath: null,
              defaultExists: false,
              path: null,
              effectivePath: null,
              exists: false,
            },
          ],
        }),
      );
      await fireEvent.click(screen.getByRole("button", { name: "Install from folder…" }));
      const dialog = within(await screen.findByRole("dialog"));
      expect(
        dialog.getByText(/May read the folder you choose: BSManager instances folder/),
      ).toBeVisible();
      expect(dialog.getByText("Where your instances are.")).toBeVisible();
      expect(dialog.queryByText("No extra permissions")).toBeNull();
    });
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

const registryPlugin = (pinned = false) =>
  makePlugin({ id: "ember", name: "Ember Pack", registry: { version: "1.0.0", pinned } });

describe("PluginsSection: registry plugins", () => {
  it("says where a registry plugin comes from and whether it is pinned", async () => {
    await show(registryPlugin(true), makePlugin({ id: "local", name: "Local" }));
    expect(row("Ember Pack").getByText("From the registry · v1.0.0")).toBeInTheDocument();
    expect(row("Ember Pack").getByText("Pinned")).toBeInTheDocument();
    expect(row("Local").queryByText(/From the registry/)).toBeNull();
    expect(row("Local").queryByText("Pinned")).toBeNull();
  });

  it("offers the update from the row and installs it after the confirmation", async () => {
    vi.mocked(api.pluginUpdates).mockResolvedValue([makePluginUpdate()]);
    await store.loadPluginUpdates();
    await show(registryPlugin());
    vi.mocked(api.previewRegistryPlugin).mockResolvedValue({
      preview: makePreview({
        id: "ember",
        name: "Ember Pack",
        version: "1.1.0",
        installedVersion: "1.0.0",
      }),
      released: null,
      notes: null,
      size: 1024,
      pinned: false,
      downgrade: false,
      changes: NO_CHANGES,
    });
    vi.mocked(api.installRegistryPlugin).mockResolvedValue(
      makePlugin({ id: "ember", name: "Ember Pack", providers: [] }),
    );
    expect(screen.getByRole("button", { name: /^Browse plugins…/ })).toHaveTextContent("1");
    await fireEvent.click(row("Ember Pack").getByRole("button", { name: "Update to v1.1.0" }));
    expect(api.previewRegistryPlugin).toHaveBeenCalledWith("ember", "1.1.0");
    const dialog = await screen.findByRole("dialog");
    await fireEvent.click(within(dialog).getByRole("button", { name: "Install" }));
    await waitFor(() => expect(api.installRegistryPlugin).toHaveBeenCalledWith("ember", "1.1.0"));
    await waitFor(() => expect(toasts()).toContain("Installed Ember Pack v1.1.0."));
  });

  it("shows no update button without an update, and the browse button without a badge", async () => {
    vi.mocked(api.pluginUpdates).mockResolvedValue([]);
    await show(registryPlugin());
    expect(row("Ember Pack").queryByRole("button", { name: /Update to/ })).toBeNull();
    expect(screen.getByRole("button", { name: "Browse plugins…" })).toHaveTextContent(
      /^Browse plugins…$/,
    );
  });

  it("opens the plugin browser", async () => {
    await show();
    vi.mocked(api.fetchPluginRegistry).mockResolvedValue({
      offline: false,
      fetchedAt: null,
      plugins: [],
    });
    await fireEvent.click(screen.getByRole("button", { name: "Browse plugins…" }));
    expect(await screen.findByRole("dialog", { name: "Plugin browser" })).toBeInTheDocument();
  });
});
