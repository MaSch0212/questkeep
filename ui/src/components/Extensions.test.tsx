import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { Show } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as app from "../lib/api";
import { createApi } from "../lib/extensions/api";
import { normalizeSettings } from "../lib/extensions/declaredSettings";
import { extensionStates, setImporter, syncExtensions } from "../lib/extensions/host";
import {
  disposePlugin,
  findPage,
  registerSettings,
  resetRegistry,
} from "../lib/extensions/registry";
import { createSettingsController } from "../lib/extensions/settings";
import type { QuestkeepApi } from "../lib/extensions/types";
import { store } from "../lib/store";
import { byTooltip, makeGame, makePlugin } from "../test-utils";
import { ExtensionDialogs } from "./ExtensionDialogs";
import { GameContextMenu } from "./GameContextMenu";
import { MultiContextMenu } from "./MultiContextMenu";
import { PluginPage } from "./PluginPage";
import { Sidebar } from "./Sidebar";
import { TitleBar } from "./TitleBar";
import { PluginSettings, pluginsWithSettings } from "./settings/PluginSettings";
import { PluginsSection } from "./settings/PluginsSection";
import { SettingsPage } from "./settings/SettingsPage";

vi.mock("../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn<() => void>() }));
vi.mock("@tauri-apps/plugin-opener", () => ({
  openPath: vi.fn<() => void>(),
  openUrl: vi.fn<() => void>(),
}));

const ICON = '<svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="9"/></svg>';
const DECLARED = [
  { key: "greet", label: "Say hello", hint: "On start.", type: "toggle", default: true },
  { key: "name", label: "Your name", type: "text", default: "" },
  { key: "size", label: "Size", type: "number", default: 3, min: 1, max: 9 },
  {
    key: "pool",
    label: "Pick from",
    type: "select",
    default: "installed",
    options: [
      { value: "installed", label: "Installed" },
      { value: "all", label: "All" },
    ],
  },
  { key: "tint", label: "Tint", type: "color", default: "#112233" },
];

/** An api for the plugin "ext" with the declared settings above, registered like the host does. */
function makeApi(stored: Record<string, unknown> = {}): QuestkeepApi {
  const settings = createSettingsController({
    declared: normalizeSettings(DECLARED),
    initial: stored,
    save: (values) => app.savePluginSettings("ext", values),
  });
  registerSettings("ext", settings);
  return createApi({ id: "ext", name: "Ext", version: "1.0.0" }, settings);
}

async function withPlugins(...plugins: ReturnType<typeof makePlugin>[]) {
  vi.mocked(app.listPlugins).mockResolvedValue(plugins);
  await store.loadPlugins();
}

const ext = () =>
  makePlugin({
    id: "ext",
    name: "Ext",
    providers: [],
    hasExtension: true,
    extension: { main: "main.js" },
  });

let errors: ReturnType<typeof vi.spyOn>;
beforeEach(() => {
  vi.clearAllMocks();
  store.reset();
  vi.mocked(app.savePluginSettings).mockResolvedValue(undefined);
  vi.mocked(app.launchProfiles).mockResolvedValue({ profiles: [], defaultId: null, playId: null });
  errors = vi.spyOn(console, "error").mockImplementation(() => undefined);
});
afterEach(() => {
  errors.mockRestore();
  setImporter(null);
  resetRegistry();
});

describe("title bar buttons", () => {
  it("shows an extension's button before Settings and calls it with the button", async () => {
    const api = makeApi();
    const onClick = vi.fn<(anchor: HTMLElement) => void>();
    api.ui.addTitleBarButton({ id: "dice", label: "Surprise me", icon: ICON, onClick });
    render(() => <TitleBar onAdd={() => undefined} />);

    const button = screen.getByRole("button", { name: "Surprise me" });
    expect(button).toHaveClass("titlebar__tool");
    expect(byTooltip("Surprise me")).toBe(button);
    expect(button.querySelector(".ext-icon svg")).not.toBeNull();
    const settings = screen.getByRole("button", { name: "Settings" });
    expect(button.compareDocumentPosition(settings) & Node.DOCUMENT_POSITION_FOLLOWING).toBe(
      Node.DOCUMENT_POSITION_FOLLOWING,
    );
    expect(button.tabIndex).toBe(0);

    await fireEvent.click(button);
    expect(onClick).toHaveBeenCalledWith(button);

    disposePlugin("ext");
    expect(screen.queryByRole("button", { name: "Surprise me" })).toBeNull();
  });

  it("shows a badge and reads it again after events and refresh()", () => {
    const api = makeApi();
    let count: number | null = null;
    const handle = api.ui.addTitleBarButton({
      id: "inbox",
      label: "Inbox",
      icon: ICON,
      onClick: () => undefined,
      badge: () => count,
    });
    render(() => <TitleBar onAdd={() => undefined} />);
    expect(document.querySelector(".titlebar__ext-badge")).toBeNull();
    count = 3;
    handle.refresh();
    expect(document.querySelector(".titlebar__ext-badge")).toHaveTextContent("3");
    expect(screen.getByRole("button", { name: "Inbox (3)" })).toBeInTheDocument();
    count = 4;
    store.handleCoreEvent({ type: "categoriesChanged" });
    expect(document.querySelector(".titlebar__ext-badge")).toHaveTextContent("4");
  });

  it("keeps working when a button's click handler throws", async () => {
    const api = makeApi();
    api.ui.addTitleBarButton({
      id: "bad",
      label: "Bad",
      icon: ICON,
      onClick: () => {
        throw new Error("boom");
      },
    });
    render(() => <TitleBar onAdd={() => undefined} />);
    await fireEvent.click(screen.getByRole("button", { name: "Bad" }));
    expect(errors).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: "Settings" })).toBeInTheDocument();
  });

  it("adds extension entries to the Questkeep menu above Settings, and to the + menu", async () => {
    const api = makeApi();
    const open = vi.fn<() => void>();
    api.ui.addMenuItems("app", () => [{ label: "Library facts", icon: ICON, onSelect: open }]);
    api.ui.addMenuItems("add", () => [{ label: "Import from file", onSelect: open }]);
    render(() => <TitleBar onAdd={() => undefined} />);

    await fireEvent.click(screen.getByRole("button", { name: "Questkeep menu" }));
    const items = screen.getAllByRole("menuitem").map((item) => item.textContent);
    expect(items.indexOf("Library facts")).toBeGreaterThan(-1);
    expect(items.indexOf("Library facts")).toBeLessThan(items.indexOf("Settings"));
    await fireEvent.click(screen.getByRole("menuitem", { name: "Library facts" }));
    expect(open).toHaveBeenCalledTimes(1);

    await fireEvent.click(screen.getByRole("button", { name: "Add game" }));
    expect(screen.getByRole("menuitem", { name: "Import from file" })).toBeInTheDocument();
  });
});

describe("context menus", () => {
  it("shows an extension's items for the game, with a submenu", async () => {
    const api = makeApi();
    const copied = vi.fn<(name: string) => void>();
    api.ui.addMenuItems("game", ({ game, games }) => [
      { label: "Copy name", icon: ICON, onSelect: () => copied(`${game?.name}/${games?.length}`) },
      { label: "More from Ext", items: [{ label: "Inner", onSelect: () => copied("inner") }] },
    ]);
    api.ui.addMenuItems("game", () => {
      throw new Error("broken provider");
    });
    render(() => (
      <GameContextMenu
        game={makeGame({ name: "Hades" })}
        anchor={{ x: 0, y: 0 }}
        onDone={() => undefined}
      />
    ));
    const labels = screen.getAllByRole("menuitem").map((item) => item.textContent);
    const removeAt = labels.findIndex((label) => label?.startsWith("Remove from library"));
    expect(removeAt).toBeGreaterThan(-1);
    expect(labels.indexOf("Copy name")).toBeGreaterThan(-1);
    expect(labels.indexOf("Copy name")).toBeLessThan(removeAt);
    expect(screen.getByRole("menuitem", { name: /More from Ext/ })).toHaveAttribute(
      "aria-haspopup",
      "menu",
    );
    await fireEvent.click(screen.getByRole("menuitem", { name: "Copy name" }));
    expect(copied).toHaveBeenCalledWith("Hades/1");
  });

  it("shows items for a multi-selection", async () => {
    const api = makeApi();
    const picked = vi.fn<(count: number) => void>();
    api.ui.addMenuItems("games", ({ games }) => [
      { label: "Export selection", onSelect: () => picked(games?.length ?? 0) },
    ]);
    render(() => (
      <MultiContextMenu
        games={[makeGame({ id: "a" }), makeGame({ id: "b" })]}
        anchor={{ x: 0, y: 0 }}
        onDone={() => undefined}
      />
    ));
    await fireEvent.click(screen.getByRole("menuitem", { name: "Export selection" }));
    expect(picked).toHaveBeenCalledWith(2);
  });
});

describe("plugin settings", () => {
  it("draws the declared settings with their values and saves changes", async () => {
    const api = makeApi({ size: 5 });
    const heard = vi.fn<(key: string, value: unknown) => void>();
    api.settings.onChange(heard);
    render(() => <PluginSettings plugin="ext" />);

    const greet = screen.getByRole("checkbox", { name: "Say hello" });
    expect(greet).toBeChecked();
    expect(greet).toHaveAccessibleDescription("On start.");
    expect(screen.getByRole("spinbutton", { name: "Size" })).toHaveValue(5);
    expect(screen.getByRole("combobox", { name: "Pick from" })).toHaveValue("installed");
    expect(screen.getByLabelText("Tint")).toHaveValue("#112233");

    await fireEvent.click(greet);
    await waitFor(() =>
      expect(app.savePluginSettings).toHaveBeenLastCalledWith("ext", { size: 5, greet: false }),
    );
    expect(heard).toHaveBeenLastCalledWith("greet", false);

    await fireEvent.change(screen.getByRole("combobox", { name: "Pick from" }), {
      target: { value: "all" },
    });
    expect(api.settings.get("pool")).toBe("all");
    await fireEvent.change(screen.getByRole("textbox", { name: "Your name" }), {
      target: { value: "Ada" },
    });
    expect(api.settings.get("name")).toBe("Ada");
    await fireEvent.change(screen.getByRole("textbox", { name: "Tint as a hex value" }), {
      target: { value: "#ABC" },
    });
    expect(api.settings.get("tint")).toBe("#aabbcc");
    expect(screen.getByLabelText("Tint")).toHaveValue("#aabbcc");
  });

  it("clamps numbers and refuses what is not one", async () => {
    const api = makeApi();
    render(() => <PluginSettings plugin="ext" />);
    const size = screen.getByRole("spinbutton", { name: "Size" });
    await fireEvent.change(size, { target: { value: "40" } });
    expect(api.settings.get("size")).toBe(9);
    expect(size).toHaveValue(9);
    const tint = screen.getByRole("textbox", { name: "Tint as a hex value" });
    await fireEvent.change(tint, { target: { value: "blue" } });
    expect(tint).toHaveValue("#112233");
    expect(api.settings.get("tint")).toBe("#112233");
  });

  it("resets one setting to its default", async () => {
    const api = makeApi({ size: 5 });
    render(() => <PluginSettings plugin="ext" />);
    const reset = screen.getByRole("button", { name: "Reset Size to its default" });
    expect(reset).toBeEnabled();
    expect(screen.getByRole("button", { name: "Reset Say hello to its default" })).toBeDisabled();
    await fireEvent.click(reset);
    expect(api.settings.get("size")).toBe(3);
    expect(screen.getByRole("spinbutton", { name: "Size" })).toHaveValue(3);
    expect(app.savePluginSettings).toHaveBeenLastCalledWith("ext", {});
    expect(reset).toBeDisabled();
  });

  it("follows values the extension sets itself and reports a failed save", async () => {
    const api = makeApi();
    render(() => <PluginSettings plugin="ext" />);
    await api.settings.set("size", 7);
    expect(screen.getByRole("spinbutton", { name: "Size" })).toHaveValue(7);
    vi.mocked(app.savePluginSettings).mockRejectedValueOnce("disk full");
    await fireEvent.click(screen.getByRole("checkbox", { name: "Say hello" }));
    await waitFor(() =>
      expect(store.toasts().map((t) => t.message)).toEqual([
        "The setting could not be saved: disk full",
      ]),
    );
  });

  it("mounts the extension's own sections and cleans them up", () => {
    const api = makeApi();
    const cleanup = vi.fn<() => void>();
    api.ui.addSettingsSection({
      id: "preview",
      title: "Preview",
      render(container) {
        container.textContent = "Live preview";
        return cleanup;
      },
    });
    api.ui.addSettingsSection({
      id: "broken",
      title: "Broken",
      render() {
        throw new Error("cannot draw");
      },
    });
    render(() => <PluginSettings plugin="ext" />);
    expect(screen.getByRole("heading", { name: "Preview" })).toBeInTheDocument();
    expect(screen.getByText("Live preview")).toHaveClass("ext-mount");
    expect(screen.getByRole("alert")).toHaveTextContent(
      "This part of the plugin could not be shown: cannot draw",
    );
    disposePlugin("ext");
    expect(cleanup).toHaveBeenCalledTimes(1);
    expect(screen.queryByText("Live preview")).toBeNull();
  });

  it("gets a section of its own on the settings page, named after the plugin", async () => {
    vi.mocked(app.getSettings).mockRejectedValue("not needed");
    await withPlugins(ext(), makePlugin({ id: "plain", name: "Plain" }));
    expect(pluginsWithSettings()).toEqual([]);
    makeApi();
    expect(pluginsWithSettings()).toEqual(["ext"]);
    render(() => <SettingsPage />);
    const nav = within(screen.getByRole("navigation", { name: "Settings sections" }));
    expect(nav.getByRole("button", { name: "Ext" })).toBeInTheDocument();
    expect(nav.queryByRole("button", { name: "Plain" })).toBeNull();
    const section = document.getElementById("settings-plugin-ext");
    expect(section).not.toBeNull();
    expect(within(section as HTMLElement).getByRole("heading", { name: "Ext" })).toHaveClass(
      "settings-section__title",
    );
    expect(
      within(section as HTMLElement).getByRole("checkbox", { name: "Say hello" }),
    ).toBeInTheDocument();
    disposePlugin("ext");
    expect(document.getElementById("settings-plugin-ext")).toBeNull();
  });
});

describe("plugin pages", () => {
  it("lists pages in the sidebar under Plugins and opens them", async () => {
    render(() => <Sidebar />);
    expect(screen.queryByRole("heading", { name: "Plugins" })).toBeNull();
    const api = makeApi();
    api.ui.addPage({ id: "facts", title: "Library facts", icon: ICON, render: () => undefined });
    api.ui.addPage({ id: "plain", title: "No icon", render: () => undefined });
    expect(screen.getByRole("heading", { name: "Plugins" })).toBeInTheDocument();
    const item = screen.getByRole("button", { name: "Library facts" });
    expect(item.querySelector(".ext-icon svg")).not.toBeNull();
    expect(
      screen.getByRole("button", { name: "No icon" }).querySelector("svg.icon"),
    ).not.toBeNull();

    await fireEvent.click(item);
    expect(store.page()).toBe("plugin:ext/facts");
    expect(item).toHaveAttribute("aria-current", "page");

    disposePlugin("ext");
    expect(screen.queryByRole("heading", { name: "Plugins" })).toBeNull();
  });

  it("renders the page with its title and cleans up when it goes", () => {
    const api = makeApi();
    const cleanup = vi.fn<() => void>();
    api.ui.addPage({
      id: "facts",
      title: "Library facts",
      render(container) {
        container.append("42 games");
        return cleanup;
      },
    });
    render(() => (
      <Show when={findPage("plugin:ext/facts")} keyed>
        {(entry) => <PluginPage entry={entry} />}
      </Show>
    ));
    expect(screen.getByRole("heading", { level: 1, name: "Library facts" })).toBeInTheDocument();
    expect(document.querySelector(".plugin-page .ext-mount")).toHaveTextContent("42 games");
    expect(cleanup).not.toHaveBeenCalled();
    disposePlugin("ext");
    expect(cleanup).toHaveBeenCalledTimes(1);
    expect(document.querySelector(".plugin-page")).toBeNull();
  });

  it("cannot be opened once its plugin is gone, also not through the history", async () => {
    const api = makeApi();
    const page = api.ui.addPage({ id: "facts", title: "Facts", render: () => undefined });
    page.open();
    expect(store.page()).toBe("plugin:ext/facts");
    store.showSettings();
    disposePlugin("ext");
    page.open();
    store.showPluginPage("plugin:ext/facts");
    expect(store.page()).toBe("settings");
  });
});

describe("dialogs and popup menus", () => {
  it("opens a dialog, closes it with Escape and runs the cleanup", async () => {
    const api = makeApi();
    const cleanup = vi.fn<() => void>();
    render(() => <ExtensionDialogs />);
    api.ui.showDialog({
      title: "Surprise me",
      width: "small",
      render(container) {
        const button = document.createElement("button");
        button.textContent = "Another one";
        container.append(button);
        return cleanup;
      },
    });
    const dialog = screen.getByRole("dialog", { name: "Surprise me" });
    expect(dialog).toHaveClass("modal--narrow");
    expect(within(dialog).getByRole("button", { name: "Another one" })).toBeInTheDocument();
    await fireEvent.keyDown(dialog, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(cleanup).toHaveBeenCalledTimes(1);
  });

  it("closes through the close button, the render's close and the handle", async () => {
    const api = makeApi();
    render(() => <ExtensionDialogs />);
    let closeFromInside: (() => void) | undefined;
    api.ui.showDialog({
      title: "One",
      render: (_container, close) => {
        closeFromInside = close;
      },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Close dialog" }));
    expect(screen.queryByRole("dialog")).toBeNull();

    api.ui.showDialog({
      title: "Two",
      render: (_container, close) => {
        closeFromInside = close;
      },
    });
    closeFromInside?.();
    expect(screen.queryByRole("dialog")).toBeNull();

    const handle = api.ui.showDialog({ title: "Three", width: "large", render: () => undefined });
    expect(screen.getByRole("dialog", { name: "Three" })).toHaveClass("modal--wide");
    handle.close();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("closes open dialogs when the plugin is unloaded", () => {
    const api = makeApi();
    render(() => <ExtensionDialogs />);
    api.ui.showDialog({ title: "Open", render: () => undefined });
    disposePlugin("ext");
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("opens a popup menu under an element", async () => {
    const api = makeApi();
    const chosen = vi.fn<() => void>();
    render(() => <ExtensionDialogs />);
    const anchor = document.createElement("button");
    document.body.append(anchor);
    api.ui.showMenu(anchor, [
      { label: "First", onSelect: chosen },
      { separator: true },
      { label: "Second", disabled: true, onSelect: chosen },
    ]);
    expect(screen.getByRole("menu", { name: "ext menu" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Second" })).toBeDisabled();
    await fireEvent.click(screen.getByRole("menuitem", { name: "First" }));
    expect(chosen).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("menu")).toBeNull();
    anchor.remove();
  });
});

describe("plugin list", () => {
  it("shows why an extension failed in its plugin's row", async () => {
    vi.mocked(app.getPluginSettings).mockResolvedValue({});
    vi.mocked(app.pluginFileUrl).mockReturnValue("qk://ext/main.js");
    setImporter(() =>
      Promise.resolve({
        activate: () => {
          throw new Error("api.ui.addSidebar is not a function");
        },
      }),
    );
    await withPlugins(ext());
    await syncExtensions(store.plugins());
    expect(extensionStates().ext?.status).toBe("failed");
    render(() => <PluginsSection />);
    const row = within(screen.getByRole("group", { name: "Ext" }));
    expect(row.getByText("Extension")).toBeInTheDocument();
    expect(row.getByText("Extension failed: api.ui.addSidebar is not a function")).toHaveClass(
      "plugin__error",
    );
  });
});
