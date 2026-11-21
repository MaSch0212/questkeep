import { describe, expect, it } from "vitest";
import { makePlugin } from "../test-utils";
import type { PluginFolder } from "../bindings/PluginFolder";
import {
  applyFolderChoice,
  folderMissing,
  folderNeedsChoice,
  folderState,
  folderUnusable,
  foldersToChoose,
  hasPermissions,
  installFolderMap,
  pluginIconPath,
  pluginKinds,
  pluginStatus,
  sortPlugins,
  withFolderChoice,
} from "./plugins";

const folder = (overrides: Partial<PluginFolder> = {}): PluginFolder => ({
  key: "games",
  label: "Games",
  hint: null,
  mount: "/games",
  default: null,
  defaultPath: null,
  defaultExists: false,
  path: null,
  effectivePath: null,
  exists: false,
  ...overrides,
});

describe("plugins", () => {
  it("builds the icon path with the load counter, or none", () => {
    expect(pluginIconPath({ icon: "img/logo.svg", revision: 4 })).toBe("@4/img/logo.svg");
    expect(pluginIconPath({ icon: "img\\logo.png", revision: 1 })).toBe("@1/img/logo.png");
    expect(pluginIconPath({ icon: null, revision: 4 })).toBeNull();
    expect(pluginIconPath({ icon: " ", revision: 4 })).toBeNull();
  });

  it("lists what a plugin brings", () => {
    expect(pluginKinds(makePlugin())).toEqual(["store"]);
    expect(pluginKinds(makePlugin({ providers: [], hasExtension: true, themes: 2 }))).toEqual([
      "extension",
      "theme",
    ]);
    expect(pluginKinds(makePlugin({ providers: [] }))).toEqual([]);
  });

  it("tells loaded, disabled and failed plugins apart", () => {
    expect(pluginStatus(makePlugin())).toBe("loaded");
    expect(pluginStatus(makePlugin({ enabled: false, loaded: false }))).toBe("disabled");
    expect(pluginStatus(makePlugin({ enabled: false, loaded: false, error: "x" }))).toBe(
      "disabled",
    );
    expect(pluginStatus(makePlugin({ loaded: false, error: "bad wasm" }))).toBe("failed");
    expect(pluginStatus(makePlugin({ loaded: false }))).toBe("failed");
  });

  it("sorts by name, ignoring case", () => {
    const list = [
      makePlugin({ id: "b", name: "beta" }),
      makePlugin({ id: "a", name: "Alpha" }),
      makePlugin({ id: "c", name: "Alpha" }),
    ];
    expect(sortPlugins(list).map((p) => p.id)).toEqual(["a", "c", "b"]);
    expect(list[0]?.id).toBe("b");
  });

  it("knows whether a plugin asks for extra access", () => {
    expect(hasPermissions(makePlugin())).toBe(false);
    expect(
      hasPermissions(
        makePlugin({
          permissions: {
            read: [],
            http: ["a.com"],
            folders: [],
            filesystem: false,
            registry: false,
            launcher: [],
            login: [],
          },
        }),
      ),
    ).toBe(true);
    const asking = { key: "g", label: "Games", hint: null, default: null };
    expect(
      hasPermissions(
        makePlugin({
          permissions: {
            read: [],
            http: [],
            folders: [asking],
            filesystem: false,
            registry: false,
            launcher: [],
            login: [],
          },
        }),
      ),
    ).toBe(true);
  });

  it("tells chosen, default and missing folders apart", () => {
    expect(folderState(folder())).toBe("unset");
    expect(folderState(folder({ effectivePath: "C:/D", exists: true }))).toBe("default");
    expect(folderState(folder({ path: "D:/G", effectivePath: "D:/G", exists: true }))).toBe(
      "chosen",
    );
    expect(folderMissing(folder())).toBe(false);
    expect(folderMissing(folder({ effectivePath: "C:/D", exists: true }))).toBe(false);
    expect(folderMissing(folder({ effectivePath: "C:/D", exists: false }))).toBe(true);
    expect(folderMissing(folder({ path: "D:/G", effectivePath: "D:/G" }))).toBe(true);
  });

  it("finds the folders nobody can see yet", () => {
    const here = folder({
      defaultPath: "C:/D",
      defaultExists: true,
      effectivePath: "C:/D",
      exists: true,
    });
    const gone = folder({ key: "gone", defaultPath: "C:/G", effectivePath: "C:/G" });
    const none = folder({ key: "none" });
    const chosen = folder({ key: "chosen", path: "D:/X", effectivePath: "D:/X", exists: true });
    expect(folderNeedsChoice(here)).toBe(false);
    expect(folderNeedsChoice(gone)).toBe(true);
    expect(folderNeedsChoice(none)).toBe(true);
    expect(folderNeedsChoice(chosen)).toBe(false);
    expect(foldersToChoose({ folders: [here, gone, none, chosen] }).map((f) => f.key)).toEqual([
      "gone",
      "none",
    ]);
    expect(folderUnusable(here)).toBe(false);
    // A missing default and "not set" both leave the plugin without a folder.
    expect(folderUnusable(gone)).toBe(true);
  });

  it("shows a choice made in the install dialog, or the default again", () => {
    const base = folder({
      defaultPath: "C:/D",
      defaultExists: true,
      effectivePath: "C:/D",
      exists: true,
    });
    expect(applyFolderChoice(base, {})).toBe(base);
    expect(applyFolderChoice(base, { other: "E:/O" })).toBe(base);
    const picked = applyFolderChoice(base, { games: "E:/G" });
    expect(picked).toMatchObject({ path: "E:/G", effectivePath: "E:/G", exists: true });
    expect(folderState(picked)).toBe("chosen");
    // An update that had a choice, reset to the default.
    const installed = folder({
      defaultPath: "C:/D",
      defaultExists: false,
      path: "D:/G",
      effectivePath: "D:/G",
      exists: true,
    });
    const reset = applyFolderChoice(installed, { games: null });
    expect(reset).toMatchObject({ path: null, effectivePath: "C:/D", exists: false });
    expect(folderMissing(reset)).toBe(true);
  });

  it("keeps only real changes as choices", () => {
    const fresh = folder();
    const installed = folder({ path: "D:/G" });
    expect(withFolderChoice({}, fresh, "E:/G")).toEqual({ games: "E:/G" });
    // Picking again replaces; leaving other folders alone.
    expect(withFolderChoice({ games: "E:/G", mods: "E:/M" }, fresh, "F:/G")).toEqual({
      games: "F:/G",
      mods: "E:/M",
    });
    // Back to the default of a fresh install: nothing to say.
    expect(withFolderChoice({ games: "E:/G" }, fresh, null)).toEqual({});
    // The installed choice again: no change; the default: a reset.
    expect(withFolderChoice({ games: "E:/G" }, installed, "D:/G")).toEqual({});
    expect(withFolderChoice({}, installed, null)).toEqual({ games: null });
  });

  it("turns the choices into the install command's folders", () => {
    expect(installFolderMap({})).toBeUndefined();
    expect(installFolderMap({ games: "E:/G", mods: null })).toEqual({ games: "E:/G", mods: "" });
  });
});
