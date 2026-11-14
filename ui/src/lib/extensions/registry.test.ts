import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  addEventListener,
  addMenuItems,
  addPage,
  addSettingsSection,
  addTitleBarButton,
  dialogs,
  disposePlugin,
  emitCoreEvent,
  findPage,
  menuItemsFor,
  menuProviders,
  openDialog,
  openPopupMenu,
  pages,
  pluginOfPage,
  pluginPageId,
  popupMenu,
  resetRegistry,
  settingsSections,
  titleBarButtons,
  track,
  trackedCount,
} from "./registry";

const button = (id: string) => ({ id, label: id, icon: "<svg/>", onClick: () => undefined });
const page = (id: string) => ({ id, title: id, render: () => undefined });

let errors: ReturnType<typeof vi.spyOn>;
beforeEach(() => {
  resetRegistry();
  errors = vi.spyOn(console, "error").mockImplementation(() => undefined);
});
afterEach(() => errors.mockRestore());

describe("registry", () => {
  it("takes back one addition through its disposable", () => {
    const first = addTitleBarButton("a", button("one"));
    addTitleBarButton("a", button("two"));
    expect(titleBarButtons().map((entry) => entry.key)).toEqual(["a/one", "a/two"]);
    first.dispose();
    expect(titleBarButtons().map((entry) => entry.key)).toEqual(["a/two"]);
    first.dispose(); // again: nothing happens
    expect(titleBarButtons()).toHaveLength(1);
    expect(trackedCount("a")).toBe(1);
  });

  it("takes back everything a plugin added, and only that plugin's", () => {
    addTitleBarButton("a", button("b"));
    addMenuItems("a", "game", () => []);
    addSettingsSection("a", { id: "s", title: "S", render: () => undefined });
    addPage("a", page("p"));
    openDialog("a", { title: "D", render: () => undefined });
    openPopupMenu({ plugin: "a", anchor: { x: 1, y: 2 }, items: [] });
    const listener = vi.fn<() => void>();
    addEventListener("a", "*", listener);
    addTitleBarButton("b", button("keep"));

    disposePlugin("a");
    expect(titleBarButtons().map((entry) => entry.plugin)).toEqual(["b"]);
    expect(menuProviders()).toHaveLength(0);
    expect(settingsSections()).toHaveLength(0);
    expect(pages()).toHaveLength(0);
    expect(dialogs()).toHaveLength(0);
    expect(popupMenu()).toBeNull();
    emitCoreEvent({ type: "gamesChanged" });
    expect(listener).not.toHaveBeenCalled();
    expect(trackedCount("a")).toBe(0);
    disposePlugin("a"); // again: nothing happens
    expect(titleBarButtons()).toHaveLength(1);
  });

  it("keeps disposing when a cleanup throws, and logs it", () => {
    const order: string[] = [];
    track("a", () => order.push("first"));
    track("a", () => {
      throw new Error("boom");
    });
    track("a", () => order.push("last"));
    disposePlugin("a");
    expect(order).toEqual(["last", "first"]);
    expect(errors).toHaveBeenCalledTimes(1);
  });

  it("replaces an addition registered again under the same id", () => {
    addPage("a", { ...page("p"), title: "Old" });
    const second = addPage("a", { ...page("p"), title: "New" });
    expect(pages().map((entry) => entry.page.title)).toEqual(["New"]);
    second.dispose();
    expect(pages()).toHaveLength(0);
  });

  it("names plugin pages and finds them", () => {
    addPage("my-plugin", page("facts"));
    const id = pluginPageId("my-plugin", "facts");
    expect(id).toBe("plugin:my-plugin/facts");
    expect(findPage(id)?.page.id).toBe("facts");
    expect(findPage("plugin:my-plugin/other")).toBeUndefined();
    expect(pluginOfPage(id)).toBe("my-plugin");
  });

  it("closes a dialog through its handle", () => {
    const first = openDialog("a", { title: "One", render: () => undefined });
    openDialog("a", { title: "Two", render: () => undefined });
    first.close();
    first.close();
    expect(dialogs().map((entry) => entry.dialog.title)).toEqual(["Two"]);
  });

  it("collects menu items per plugin; a provider that throws adds nothing", () => {
    addMenuItems("a", "game", ({ game }) => [{ label: `A ${game?.name}`, onSelect: () => {} }]);
    addMenuItems("b", "game", () => {
      throw new Error("broken");
    });
    addMenuItems("c", "game", () => []);
    addMenuItems("d", "app", () => [{ separator: true }]);
    const game = { name: "Hades" } as never;
    expect(menuItemsFor("game", { game })).toEqual([
      { plugin: "a", items: [expect.objectContaining({ label: "A Hades" })] },
    ]);
    expect(errors).toHaveBeenCalledTimes(1);
  });

  it("hands events to matching listeners and survives one that throws", () => {
    const all = vi.fn<(event: { type: string }) => void>();
    const changed = vi.fn<(event: { type: string }) => void>();
    addEventListener("a", "gamesChanged", () => {
      throw new Error("boom");
    });
    addEventListener("a", "gamesChanged", changed);
    addEventListener("b", "*", all);
    emitCoreEvent({ type: "gamesChanged", ids: ["g1"] });
    emitCoreEvent({ type: "gameStarted", id: "g1" });
    expect(changed).toHaveBeenCalledTimes(1);
    expect(changed).toHaveBeenCalledWith({ type: "gamesChanged", ids: ["g1"] });
    expect(all).toHaveBeenCalledTimes(2);
  });
});
