import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { makeGame } from "../../test-utils";
import * as app from "../api";
import { store } from "../store";
import { createApi } from "./api";
import { normalizeSettings } from "./declaredSettings";
import { convertMenuItems, extensionMenuEntries } from "./menus";
import {
  dialogs,
  disposePlugin,
  findPage,
  popupMenu,
  resetRegistry,
  titleBarButtons,
  trackedCount,
} from "./registry";
import { createSettingsController } from "./settings";
import type { MenuItem, QuestkeepApi } from "./types";

vi.mock("../api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));

const DECLARED = normalizeSettings([
  { key: "greet", label: "Greet", type: "toggle", default: true },
  { key: "size", label: "Size", type: "number", default: 3, min: 1, max: 9 },
]);

const save = vi.fn<(settings: Record<string, unknown>) => Promise<void>>();

function makeApi(initial: Record<string, unknown> = {}): QuestkeepApi {
  const settings = createSettingsController({ declared: DECLARED, initial, save });
  return createApi({ id: "ext", name: "Ext", version: "1.2.3" }, settings);
}

const icon = () => () => null;
type Entry = { kind?: string; label?: string; onSelect?: () => void; entries?: () => Entry[] };
const shape = (entries: Entry[]): unknown[] =>
  entries.map((entry) =>
    entry.kind === "separator"
      ? "---"
      : entry.kind === "submenu"
        ? { [entry.label ?? ""]: shape(entry.entries?.() ?? []) }
        : entry.label,
  );

let errors: ReturnType<typeof vi.spyOn>;
beforeEach(() => {
  vi.clearAllMocks();
  store.reset();
  resetRegistry();
  save.mockResolvedValue(undefined);
  errors = vi.spyOn(console, "error").mockImplementation(() => undefined);
});
afterEach(() => {
  errors.mockRestore();
  resetRegistry();
  delete document.documentElement.dataset.scheme;
});

describe("api.settings", () => {
  it("falls back to the declared default, then to the given one", () => {
    const api = makeApi({ size: 5, free: "text" });
    expect(api.settings.get("size")).toBe(5);
    expect(api.settings.get("greet")).toBe(true);
    expect(api.settings.get("greet", false)).toBe(true);
    expect(api.settings.get("free")).toBe("text");
    expect(api.settings.get("unknown")).toBeUndefined();
    expect(api.settings.get("unknown", 7)).toBe(7);
    expect(api.settings.all()).toEqual({ greet: true, size: 5, free: "text" });
  });

  it("reads a stored value that no longer fits its declaration as the default", () => {
    const api = makeApi({ size: "huge", greet: "yes" });
    expect(api.settings.get("size")).toBe(3);
    expect(api.settings.get("greet")).toBe(true);
  });

  it("saves the whole object and tells the listeners", async () => {
    const api = makeApi({ free: 1 });
    const heard = vi.fn<(key: string, value: unknown) => void>();
    const listener = api.settings.onChange(heard);
    await api.settings.set("size", 4);
    expect(save).toHaveBeenLastCalledWith({ free: 1, size: 4 });
    expect(heard).toHaveBeenLastCalledWith("size", 4);
    expect(api.settings.get("size")).toBe(4);

    await api.settings.set("size", undefined); // back to the default
    expect(save).toHaveBeenLastCalledWith({ free: 1 });
    expect(heard).toHaveBeenLastCalledWith("size", 3);

    listener.dispose();
    await api.settings.set("size", 2);
    expect(heard).toHaveBeenCalledTimes(2);
  });

  it("saves one after the other, the newest object last", async () => {
    const api = makeApi();
    const order: unknown[] = [];
    let releaseFirst: (() => void) | undefined;
    save.mockImplementationOnce(
      (settings) =>
        new Promise((resolve) => {
          releaseFirst = () => {
            order.push(settings);
            resolve();
          };
        }),
    );
    save.mockImplementation((settings) => {
      order.push(settings);
      return Promise.resolve();
    });
    const first = api.settings.set("size", 1);
    const second = api.settings.set("size", 2);
    await Promise.resolve();
    expect(save).toHaveBeenCalledTimes(1);
    releaseFirst?.();
    await Promise.all([first, second]);
    expect(order).toEqual([{ size: 1 }, { size: 2 }]);
  });

  it("rejects when saving fails but keeps saving later changes", async () => {
    const api = makeApi();
    save.mockRejectedValueOnce(new Error("disk full"));
    await expect(api.settings.set("size", 1)).rejects.toThrow("disk full");
    await api.settings.set("size", 2);
    expect(save).toHaveBeenLastCalledWith({ size: 2 });
  });

  it("keeps the app alive when a listener throws", async () => {
    const api = makeApi();
    const second = vi.fn<() => void>();
    api.settings.onChange(() => {
      throw new Error("listener broke");
    });
    api.settings.onChange(second);
    await api.settings.set("size", 2);
    expect(second).toHaveBeenCalledTimes(1);
    expect(errors).toHaveBeenCalledTimes(1);
  });
});

describe("api.ui", () => {
  it("rejects registrations that could not work, naming what is wrong", () => {
    const api = makeApi();
    expect(() => api.ui.addTitleBarButton({ id: "x", label: "X" } as never)).toThrow(/icon/);
    expect(() => api.ui.addMenuItems("nowhere" as never, () => [])).toThrow(/nowhere/);
    expect(() => api.ui.addPage({ id: "p", title: "P" } as never)).toThrow(/render/);
    expect(trackedCount("ext")).toBe(0);
  });

  it("opens its page, dialogs and menus, and takes them back with the plugin", () => {
    const api = makeApi();
    const page = api.ui.addPage({ id: "facts", title: "Facts", render: () => undefined });
    page.open();
    expect(store.page()).toBe("plugin:ext/facts");
    const dialog = api.ui.showDialog({ title: "Hello", render: () => undefined });
    api.ui.showMenu({ x: 3, y: 4 }, [{ label: "One", onSelect: () => undefined }]);
    api.ui.addStyles(".example { color: red }");
    expect(dialogs()).toHaveLength(1);
    expect(popupMenu()?.items).toHaveLength(1);
    expect(document.head.querySelector("style[data-plugin=ext]")).not.toBeNull();

    dialog.close();
    expect(dialogs()).toHaveLength(0);
    disposePlugin("ext");
    expect(findPage("plugin:ext/facts")).toBeUndefined();
    expect(popupMenu()).toBeNull();
    expect(document.head.querySelector("style[data-plugin=ext]")).toBeNull();
    page.open(); // gone: stays where it is
    expect(titleBarButtons()).toHaveLength(0);
  });

  it("shows toasts", () => {
    const api = makeApi();
    api.ui.toast("Saved");
    api.ui.toast("Oops", "error");
    expect(store.toasts().map((t) => [t.kind, t.message])).toEqual([
      ["info", "Saved"],
      ["error", "Oops"],
    ]);
  });
});

describe("api.games, events, theme, invoke", () => {
  it("lists the whole library, hidden games and apps included", async () => {
    vi.mocked(app.listGames).mockResolvedValue([makeGame()]);
    const api = makeApi();
    expect((await api.games.list()).map((game) => game.name)).toEqual(["Hades"]);
    expect(app.listGames).toHaveBeenCalledWith(
      expect.objectContaining({ hidden: "all", kind: "both", sources: [], search: null }),
    );
    vi.mocked(app.listGames).mockResolvedValue(undefined as never);
    expect(await api.games.list()).toEqual([]);
  });

  it("gets one game, or undefined when there is none", async () => {
    vi.mocked(app.getGame).mockResolvedValueOnce(makeGame({ id: "g9" }));
    const api = makeApi();
    expect((await api.games.get("g9"))?.id).toBe("g9");
    vi.mocked(app.getGame).mockRejectedValueOnce("No such game");
    expect(await api.games.get("nope")).toBeUndefined();
  });

  it("reports the selection and its changes", async () => {
    vi.mocked(app.listGames).mockResolvedValue([makeGame({ id: "a" }), makeGame({ id: "b" })]);
    await store.refresh();
    const api = makeApi();
    const heard = vi.fn<(ids: string[]) => void>();
    const listener = api.games.onSelectionChange((games) => heard(games.map((g) => g.id)));
    expect(api.games.selected()).toEqual([]);
    store.selectGame("a");
    expect(heard).toHaveBeenLastCalledWith(["a"]);
    expect(api.games.selected().map((g) => g.id)).toEqual(["a"]);
    store.toggleSelect("b");
    expect(heard).toHaveBeenLastCalledWith(["a", "b"]);
    await store.refresh(); // the same games again: no change
    expect(heard).toHaveBeenCalledTimes(2);
    listener.dispose();
    store.selectGame(null);
    expect(heard).toHaveBeenCalledTimes(2);
  });

  it("opens a game's page", async () => {
    vi.mocked(app.listGames).mockResolvedValue([makeGame({ id: "a" })]);
    await store.refresh();
    makeApi().games.open("a");
    expect(store.selectedId()).toBe("a");
    expect(store.detailsOpen()).toBe(true);
  });

  it("hears core events until disposed, also when another listener throws", () => {
    const api = makeApi();
    const heard = vi.fn<(event: { type: string }) => void>();
    api.events.on("gamesChanged", () => {
      throw new Error("boom");
    });
    const listener = api.events.on("gamesChanged", heard);
    store.handleCoreEvent({ type: "gamesChanged", ids: ["g1"] });
    expect(heard).toHaveBeenCalledWith({ type: "gamesChanged", ids: ["g1"] });
    listener.dispose();
    store.handleCoreEvent({ type: "gamesChanged", ids: ["g1"] });
    expect(heard).toHaveBeenCalledTimes(1);
  });

  it("knows the color scheme and when it changes", async () => {
    const api = makeApi();
    expect(api.theme.scheme()).toBe("dark");
    const heard = vi.fn<() => void>();
    api.theme.onChange(heard);
    document.documentElement.dataset.scheme = "light";
    await vi.waitFor(() => expect(heard).toHaveBeenCalledTimes(1));
    expect(api.theme.scheme()).toBe("light");
  });

  it("passes commands and file urls through", async () => {
    vi.mocked(app.invokeCommand).mockResolvedValue(3);
    vi.mocked(app.pluginFileUrl).mockReturnValue("qk://ext/a.png");
    const api = makeApi();
    expect(await api.invoke("count_games", { queries: [] })).toBe(3);
    expect(app.invokeCommand).toHaveBeenCalledWith("count_games", { queries: [] });
    expect(api.plugin.url("a.png")).toBe("qk://ext/a.png");
    expect(app.pluginFileUrl).toHaveBeenCalledWith("ext", "a.png");
    // With the plugin's load counter, so a reloaded plugin gets its new files.
    const reloaded = createApi(
      { id: "ext", name: "Ext", version: "1", revision: 4 },
      createSettingsController({ declared: [], initial: {}, save }),
    );
    expect(reloaded.plugin.url("a.png")).toBe("qk://ext/a.png?v=4");
  });
});

describe("menu conversion", () => {
  it("turns items, submenus and separators into menu entries", () => {
    const select = vi.fn<() => void>();
    const items: MenuItem[] = [
      { separator: true },
      { label: "Plain", hint: "Ctrl+P", danger: true, checked: false, onSelect: select },
      { separator: true },
      { separator: true },
      { label: "More", icon: "<svg/>", items: [{ label: "Inner", onSelect: select }] },
      { separator: true },
    ];
    const entries = convertMenuItems("ext", items, icon);
    expect(shape(entries as Entry[])).toEqual(["Plain", "---", { More: ["Inner"] }]);
    expect(entries[0]).toMatchObject({ hint: "Ctrl+P", danger: true, checked: false });
    (entries[0] as Entry).onSelect?.();
    expect(select).toHaveBeenCalledTimes(1);
  });

  it("leaves out what is not a menu item", () => {
    const items = [null, "text", { label: "No action" }, { onSelect: () => {} }, { label: "" }];
    expect(convertMenuItems("ext", items as never, icon)).toEqual([]);
    expect(convertMenuItems("ext", "nope" as never, icon)).toEqual([]);
  });

  it("logs an item whose action throws", () => {
    const [entry] = convertMenuItems(
      "ext",
      [
        {
          label: "Boom",
          onSelect: () => {
            throw new Error("boom");
          },
        },
      ],
      icon,
    );
    expect(() => (entry as Entry).onSelect?.()).not.toThrow();
    expect(errors).toHaveBeenCalledTimes(1);
  });

  it("puts each plugin's items after a separator and passes the context on", () => {
    const api = makeApi();
    const game = makeGame({ name: "Hades" });
    api.ui.addMenuItems("game", (context) => [
      { label: `Copy ${context.game?.name}`, onSelect: () => undefined },
    ]);
    api.ui.addMenuItems("game", () => []);
    api.ui.addMenuItems("games", () => [{ label: "Many", onSelect: () => undefined }]);
    expect(shape(extensionMenuEntries("game", { game }, icon) as Entry[])).toEqual([
      "---",
      "Copy Hades",
    ]);
    expect(extensionMenuEntries("app", {}, icon)).toEqual([]);
  });
});
