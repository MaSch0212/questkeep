import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PluginInfo } from "../../bindings/PluginInfo";
import { makePlugin } from "../../test-utils";
import * as app from "../api";
import { store } from "../store";
import { extensionStates, resetExtensions, setImporter, syncExtensions } from "./host";
import { settingsControllers, titleBarButtons, trackedCount } from "./registry";
import type { ExtensionModule, QuestkeepApi } from "./types";

vi.mock("../api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));

const extension = (overrides: Partial<PluginInfo> = {}): PluginInfo =>
  makePlugin({
    id: "ext",
    name: "Ext",
    providers: [],
    hasExtension: true,
    extension: { main: "main.js", styles: ["style.css"] },
    revision: 1,
    ...overrides,
  });

const button = (label: string) => ({
  id: "b",
  label,
  icon: "<svg/>",
  onClick: () => undefined,
});

/** A module whose activate adds a title bar button named after `tag`; calls land in `log`. */
function moduleOf(tag: string, log: string[]): ExtensionModule {
  return {
    activate(api) {
      log.push(`activate ${tag}`);
      api.ui.addTitleBarButton(button(tag));
      return () => log.push(`cleanup ${tag}`);
    },
    deactivate() {
      log.push(`deactivate ${tag}`);
    },
  };
}

const labels = () => titleBarButtons().map((entry) => entry.button.label);
const links = () =>
  [...document.head.querySelectorAll<HTMLLinkElement>("link[data-plugin]")].map((l) => l.href);
const toasts = () => store.toasts().map((toast) => toast.message);

let errors: ReturnType<typeof vi.spyOn>;
beforeEach(() => {
  vi.clearAllMocks();
  store.reset();
  resetExtensions();
  vi.mocked(app.pluginFileUrl).mockImplementation((id, path) => `qk://plugins/${id}/${path}`);
  vi.mocked(app.getPluginSettings).mockResolvedValue({});
  errors = vi.spyOn(console, "error").mockImplementation(() => undefined);
});
afterEach(() => {
  errors.mockRestore();
  setImporter(null);
  resetExtensions();
});

describe("extension host", () => {
  it("imports the main module, loads the styles and activates with the plugin's api", async () => {
    let received: QuestkeepApi | undefined;
    const importer = vi.fn<(url: string) => Promise<unknown>>().mockResolvedValue({
      activate: (api: QuestkeepApi) => {
        received = api;
      },
    });
    setImporter(importer);
    await syncExtensions([extension({ version: "2.0.0" })]);

    expect(importer).toHaveBeenCalledWith("qk://plugins/ext/@1/main.js?v=1");
    expect(links()).toEqual(["qk://plugins/ext/@1/style.css?v=1"]);
    expect(received?.plugin).toMatchObject({ id: "ext", name: "Ext", version: "2.0.0" });
    expect(received?.apiVersion).toBe(1);
    expect(extensionStates()).toEqual({ ext: { status: "active" } });
    expect(settingsControllers().ext).toBeDefined();
  });

  it("only runs enabled, loaded plugins that have an extension", async () => {
    const importer = vi.fn<(url: string) => Promise<unknown>>().mockResolvedValue({
      activate: () => undefined,
    });
    setImporter(importer);
    await syncExtensions([
      extension({ id: "off", enabled: false, loaded: false }),
      extension({ id: "broken", loaded: false }),
      makePlugin({ id: "store-only" }),
    ]);
    expect(importer).not.toHaveBeenCalled();
    expect(extensionStates()).toEqual({});
    await syncExtensions(undefined);
    expect(extensionStates()).toEqual({});
  });

  it("reloads on a new revision: the old additions go, the new ones come", async () => {
    const log: string[] = [];
    setImporter((url) => Promise.resolve(moduleOf(url.endsWith("v=1") ? "one" : "two", log)));
    await syncExtensions([extension()]);
    expect(labels()).toEqual(["one"]);

    await syncExtensions([extension()]); // the same revision: nothing to do
    expect(log).toEqual(["activate one"]);

    await syncExtensions([extension({ revision: 2 })]);
    expect(log).toEqual(["activate one", "deactivate one", "cleanup one", "activate two"]);
    expect(labels()).toEqual(["two"]);
    expect(links()).toEqual(["qk://plugins/ext/@2/style.css?v=2"]);
    expect(extensionStates()).toEqual({ ext: { status: "active" } });
  });

  it("deactivates a plugin that was disabled or removed", async () => {
    const log: string[] = [];
    setImporter(() => Promise.resolve(moduleOf("one", log)));
    await syncExtensions([extension()]);
    await syncExtensions([extension({ enabled: false, loaded: false, extension: null })]);
    expect(log).toEqual(["activate one", "deactivate one", "cleanup one"]);
    expect(labels()).toEqual([]);
    expect(links()).toEqual([]);
    expect(extensionStates()).toEqual({});
    expect(settingsControllers()).toEqual({});
    expect(trackedCount("ext")).toBe(0);

    await syncExtensions([extension()]);
    expect(labels()).toEqual(["one"]);
    await syncExtensions([]);
    expect(labels()).toEqual([]);
  });

  it("marks an extension whose activate throws as failed and leaves nothing registered", async () => {
    const importer = vi.fn<(url: string) => Promise<unknown>>().mockResolvedValue({
      activate(api: QuestkeepApi) {
        api.ui.addTitleBarButton(button("half"));
        api.ui.addStyles(".x{}");
        throw new Error("no luck");
      },
    });
    setImporter(importer);
    await syncExtensions([extension()]);
    expect(extensionStates()).toEqual({ ext: { status: "failed", error: "no luck" } });
    expect(labels()).toEqual([]);
    expect(links()).toEqual([]);
    expect(document.head.querySelector("style[data-plugin]")).toBeNull();
    expect(settingsControllers()).toEqual({});
    expect(trackedCount("ext")).toBe(0);
    expect(toasts()).toEqual(["The extension of Ext could not be started: no luck"]);

    // The same revision is not tried again; a new one is.
    await syncExtensions([extension()]);
    expect(importer).toHaveBeenCalledTimes(1);
    expect(toasts()).toHaveLength(1);
    importer.mockResolvedValue({ activate: () => undefined });
    await syncExtensions([extension({ revision: 2 })]);
    expect(extensionStates()).toEqual({ ext: { status: "active" } });
  });

  it("fails on a rejected activate, a failed import and a missing activate", async () => {
    setImporter((url) => {
      if (url.includes("/rejects/")) {
        return Promise.resolve({ activate: () => Promise.reject(new Error("async no")) });
      }
      if (url.includes("/missing/")) return Promise.resolve({ default: {} });
      return Promise.reject(new Error("Failed to fetch dynamically imported module"));
    });
    await syncExtensions([
      extension({ id: "rejects" }),
      extension({ id: "missing" }),
      extension({ id: "gone" }),
      extension({ id: "nomain", extension: { styles: [] } }),
    ]);
    expect(extensionStates()).toEqual({
      rejects: { status: "failed", error: "async no" },
      missing: {
        status: "failed",
        error: "The extension's main file does not export an activate function.",
      },
      gone: { status: "failed", error: "Failed to fetch dynamically imported module" },
      nomain: {
        status: "failed",
        error: 'The plugin\'s "extension" has no usable "main" file.',
      },
    });
    expect(toasts()).toHaveLength(4);
  });

  it("accepts a default export and a disposable from activate", async () => {
    const dispose = vi.fn<() => void>();
    setImporter(() => Promise.resolve({ default: { activate: () => ({ dispose }) } }));
    await syncExtensions([extension()]);
    expect(extensionStates().ext?.status).toBe("active");
    await syncExtensions([]);
    expect(dispose).toHaveBeenCalledTimes(1);
  });

  it("survives a throwing deactivate and cleanup", async () => {
    setImporter(() =>
      Promise.resolve({
        activate: (api: QuestkeepApi) => {
          api.ui.addTitleBarButton(button("one"));
          return () => {
            throw new Error("cleanup broke");
          };
        },
        deactivate: () => {
          throw new Error("deactivate broke");
        },
      }),
    );
    await syncExtensions([extension()]);
    await syncExtensions([]);
    expect(labels()).toEqual([]);
    expect(extensionStates()).toEqual({});
  });

  it("never runs two instances when a reload arrives while the first still activates", async () => {
    const log: string[] = [];
    let release: ((module: unknown) => void) | undefined;
    const importer = vi.fn<(url: string) => Promise<unknown>>((url) =>
      url.endsWith("v=1")
        ? new Promise((resolve) => {
            release = resolve;
          })
        : Promise.resolve(moduleOf("two", log)),
    );
    setImporter(importer);

    const first = syncExtensions([extension()]);
    await vi.waitFor(() => expect(importer).toHaveBeenCalledTimes(1));
    expect(extensionStates()).toEqual({ ext: { status: "loading" } });
    const second = syncExtensions([extension({ revision: 2 })]);
    const third = syncExtensions([extension({ revision: 2 })]);
    release?.(moduleOf("one", log));
    await Promise.all([first, second, third]);

    expect(log).toEqual(["activate one", "deactivate one", "cleanup one", "activate two"]);
    expect(labels()).toEqual(["two"]);
    expect(importer).toHaveBeenCalledTimes(2);
  });

  it("skips revisions that were replaced before their turn came", async () => {
    const log: string[] = [];
    const importer = vi.fn<(url: string) => Promise<unknown>>((url) =>
      Promise.resolve(moduleOf(url.slice(-1), log)),
    );
    setImporter(importer);
    void syncExtensions([extension({ revision: 1 })]);
    void syncExtensions([extension({ revision: 2 })]);
    await syncExtensions([extension({ revision: 3 })]);
    expect(log).toEqual(["activate 3"]);
  });

  it("hands the stored settings to the extension", async () => {
    vi.mocked(app.getPluginSettings).mockResolvedValue({ greet: false, extra: 1 });
    let seen: Record<string, unknown> = {};
    setImporter(() =>
      Promise.resolve({
        activate: (api: QuestkeepApi) => {
          seen = api.settings.all();
        },
      }),
    );
    await syncExtensions([
      extension({
        extension: {
          main: "main.js",
          settings: [
            { key: "greet", label: "Greet", type: "toggle", default: true },
            { key: "size", label: "Size", type: "number", default: 3 },
          ],
        },
      }),
    ]);
    expect(app.getPluginSettings).toHaveBeenCalledWith("ext");
    expect(seen).toEqual({ greet: false, size: 3, extra: 1 });
  });
});
