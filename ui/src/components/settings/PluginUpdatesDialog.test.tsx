import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { PluginUpdate } from "../../bindings/PluginUpdate";
import type { RegistryPluginPreview } from "../../bindings/RegistryPluginPreview";
import * as api from "../../lib/api";
import { store } from "../../lib/store";
import { makePlugin, makePluginUpdate, makeRegistryPreview, NO_CHANGES } from "../../test-utils";
import { PluginUpdatesDialog } from "./PluginUpdatesDialog";

vi.mock("../../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn<() => void>() }));

const onClose = vi.fn<() => void>();
const toasts = () => store.toasts().map((t) => t.message);

const ember = makePluginUpdate({ notes: "Warmer reds." });
const bsmanager = makePluginUpdate({
  id: "bsmanager",
  name: "BSManager",
  installedVersion: "0.1.0",
  latestVersion: "0.2.0",
  notes: "Reads the new maps folder.",
});
const clock = makePluginUpdate({
  id: "clock",
  name: "Clock",
  installedVersion: "2.0.0",
  latestVersion: "2.0.1",
});

const REVIEW = {
  ...NO_CHANGES,
  newHttp: ["api.example.com"],
  needsReview: true,
};

/** The preview a registry version gets: nothing new asked unless `changes` says so. */
const previewOf = (
  update: PluginUpdate,
  changes = NO_CHANGES,
  extra: Parameters<typeof makeRegistryPreview>[0] = {},
): RegistryPluginPreview =>
  makeRegistryPreview(
    {
      id: update.id,
      name: update.name,
      version: update.latestVersion,
      installedVersion: update.installedVersion,
      permissions: { read: [], http: changes.newHttp, folders: [] },
      ...extra,
    },
    { changes, notes: update.notes },
  );

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

async function show(updates: PluginUpdate[]) {
  vi.mocked(api.pluginUpdates).mockResolvedValue(updates);
  vi.mocked(api.listPlugins).mockResolvedValue([
    makePlugin({ id: "ember", name: "Ember Pack", icon: "img/ember.svg", providers: [] }),
    makePlugin({ id: "bsmanager", name: "BSManager", providers: ["bs-store"] }),
    makePlugin({ id: "clock", name: "Clock", providers: [] }),
  ]);
  await store.loadPlugins();
  await store.loadPluginUpdates();
  render(() => <PluginUpdatesDialog onClose={onClose} />);
  return screen.getByRole("dialog", { name: "Plugin updates" });
}

const row = (id: string) => {
  const element = document.querySelector<HTMLElement>(`[data-plugin-update-row="${id}"]`);
  if (!element) throw new Error(`no row ${id}`);
  return within(element);
};

const result = (id: string) =>
  within(document.querySelector<HTMLElement>(`[data-plugin-result="${id}"]`) ?? document.body);

const updateAll = () => fireEvent.click(screen.getByRole("button", { name: "Update all" }));

beforeEach(() => {
  vi.clearAllMocks();
  store.reset();
  vi.mocked(api.pluginFileUrl).mockImplementation((id, path) => `qk://plugins/${id}/${path}`);
  vi.mocked(api.scanLibrary).mockResolvedValue({ added: 1, updated: 0, uninstalled: 0 });
  vi.mocked(api.installRegistryPlugin).mockImplementation((id) =>
    Promise.resolve(
      makePlugin({ id, name: id, providers: id === "bsmanager" ? ["bs-store"] : [] }),
    ),
  );
});

describe("PluginUpdatesDialog: the list", () => {
  it("shows a row per plugin with icon, versions, notes and an Update button", async () => {
    await show([ember, bsmanager]);
    expect(screen.getByRole("list", { name: "Available updates" })).toBeInTheDocument();
    // Sorted by name.
    expect(
      Array.from(document.querySelectorAll("[data-plugin-update-row]")).map((el) =>
        el.getAttribute("data-plugin-update-row"),
      ),
    ).toEqual(["bsmanager", "ember"]);
    expect(row("ember").getByText("Ember Pack")).toBeInTheDocument();
    expect(row("ember").getByText("1.0.0 \u2192 1.1.0")).toBeInTheDocument();
    expect(row("ember").getByText("Warmer reds.")).toBeInTheDocument();
    expect(document.querySelector('[data-plugin-update-row="ember"] img')).toHaveAttribute(
      "src",
      "qk://plugins/ember/@1/img/ember.svg",
    );
    expect(row("bsmanager").getByText("0.1.0 \u2192 0.2.0")).toBeInTheDocument();
    expect(
      row("bsmanager").getByRole("button", { name: "Update BSManager to 0.2.0" }),
    ).toBeInTheDocument();
    // The primary action has the focus; Esc closes.
    expect(screen.getByRole("button", { name: "Update all" })).toHaveFocus();
  });

  it("says so when everything is up to date, and Esc closes", async () => {
    await show([]);
    expect(screen.getByText("Everything is up to date.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Update all" })).toBeDisabled();
    await fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(onClose).toHaveBeenCalled();
  });

  it("updates one plugin through the usual install dialog", async () => {
    await show([ember, bsmanager]);
    vi.mocked(api.previewRegistryPlugin).mockResolvedValue(previewOf(ember));
    await fireEvent.click(row("ember").getByRole("button", { name: "Update Ember Pack to 1.1.0" }));
    expect(api.previewRegistryPlugin).toHaveBeenCalledWith("ember", "1.1.0");
    const install = within(await screen.findByRole("dialog", { name: "Install this plugin?" }));
    vi.mocked(api.pluginUpdates).mockResolvedValue([bsmanager]);
    await fireEvent.click(install.getByRole("button", { name: "Install" }));
    await waitFor(() => expect(api.installRegistryPlugin).toHaveBeenCalledWith("ember", "1.1.0"));
    await waitFor(() => expect(toasts()).toContain("Installed ember v1.1.0."));
    // The list follows the update count.
    await waitFor(() =>
      expect(document.querySelector('[data-plugin-update-row="ember"]')).toBeNull(),
    );
    expect(row("bsmanager").getByText("0.1.0 \u2192 0.2.0")).toBeInTheDocument();
  });

  it("shows the permission change when a single update needs review", async () => {
    await show([bsmanager]);
    vi.mocked(api.previewRegistryPlugin).mockResolvedValue(
      previewOf(bsmanager, REVIEW, { stores: 1 }),
    );
    await fireEvent.click(
      row("bsmanager").getByRole("button", { name: "Update BSManager to 0.2.0" }),
    );
    const install = within(await screen.findByRole("dialog", { name: "Install this plugin?" }));
    expect(install.getByText("api.example.com")).toBeInTheDocument();
    expect(install.getByText("May contact these web hosts")).toBeInTheDocument();
    expect(
      install.getByText("Needs your review: asks for web access to api.example.com."),
    ).toBeInTheDocument();
  });
});

async function flagBsmanager() {
  await show([bsmanager, clock]);
  vi.mocked(api.previewRegistryPlugin).mockImplementation((id) => {
    const found = [bsmanager, clock].find((u) => u.id === id);
    if (!found) return Promise.reject(new Error("unknown"));
    return Promise.resolve(previewOf(found, id === "bsmanager" ? REVIEW : NO_CHANGES));
  });
  await updateAll();
  await fireEvent.click(await screen.findByRole("button", { name: "Update 1 plugin" }));
  await screen.findByRole("heading", { name: "Results" });
}

describe("PluginUpdatesDialog: updates that need review", () => {
  it("offers the normal Update button on a result row that needs review, with a hint", async () => {
    await flagBsmanager();
    expect(
      result("bsmanager").getByText(
        "Not updated: needs your review (asks for web access to api.example.com).",
      ),
    ).toBeInTheDocument();
    expect(
      result("bsmanager").getByText("Use Update for this plugin instead of Update all."),
    ).toBeInTheDocument();
    expect(
      result("bsmanager").getByRole("button", { name: "Update BSManager to 0.2.0" }),
    ).toBeEnabled();
    // Rows that were updated have no button and no hint.
    expect(result("clock").queryByRole("button")).toBeNull();
    expect(result("clock").queryByText(/instead of Update all/)).toBeNull();
    // The removed "Open in Settings" link is gone.
    expect(screen.queryByRole("button", { name: /in Settings/ })).toBeNull();
    expect("showPluginInSettings" in store).toBe(false);
    expect("pluginTarget" in store).toBe(false);
    expect("takePluginTarget" in store).toBe(false);
  });

  it("runs the single-plugin flow from the result row and turns it into Updated", async () => {
    await flagBsmanager();
    vi.mocked(api.pluginUpdates).mockResolvedValue([]);
    await fireEvent.click(
      result("bsmanager").getByRole("button", { name: "Update BSManager to 0.2.0" }),
    );
    const install = within(await screen.findByRole("dialog", { name: "Install this plugin?" }));
    expect(
      install.getByText("Needs your review: asks for web access to api.example.com."),
    ).toBeInTheDocument();
    await fireEvent.click(install.getByRole("button", { name: "Install" }));
    await waitFor(() =>
      expect(api.installRegistryPlugin).toHaveBeenCalledWith("bsmanager", "0.2.0"),
    );
    expect(await result("bsmanager").findByText("Updated to 0.2.0")).toBeInTheDocument();
    expect(result("bsmanager").queryByRole("button")).toBeNull();
    expect(result("bsmanager").queryByText(/instead of Update all/)).toBeNull();
    expect(screen.getByText("Updated: 2 · Failed: 0 · To review: 0")).toBeInTheDocument();
    await waitFor(() => expect(toasts()).toContain("Installed bsmanager v0.2.0."));
  });

  it("is disabled while an update is being prepared or confirmed", async () => {
    await flagBsmanager();
    const second = deferred<RegistryPluginPreview>();
    vi.mocked(api.previewRegistryPlugin).mockReturnValue(second.promise);
    const button = result("bsmanager").getByRole("button", { name: "Update BSManager to 0.2.0" });
    await fireEvent.click(button);
    await waitFor(() => expect(button).toBeDisabled());
    expect(button).toHaveAttribute("aria-busy", "true");
    second.resolve(previewOf(bsmanager, REVIEW));
    await screen.findByRole("dialog", { name: "Install this plugin?" });
    // The install dialog is open: nothing else can be started.
    expect(button).toBeDisabled();
  });

  it("marks a list row with the review explanation, the hint and its own button", async () => {
    await flagBsmanager();
    await fireEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(
      row("bsmanager").getByText("Needs your review: asks for web access to api.example.com."),
    ).toBeInTheDocument();
    expect(
      row("bsmanager").getByText("Update all skips it: use its Update button."),
    ).toBeInTheDocument();
    expect(
      row("bsmanager").getByRole("button", { name: "Update BSManager to 0.2.0" }),
    ).toBeEnabled();
    expect(row("clock").queryByText(/Update all skips it/)).toBeNull();
  });

  it("shows no hint before anything was checked", async () => {
    await show([bsmanager]);
    expect(screen.queryByText(/Update all skips it/)).toBeNull();
  });
});

describe("PluginUpdatesDialog: Update all", () => {
  it("downloads one after the other and installs only what asks for nothing new", async () => {
    await show([ember, bsmanager, clock]);
    vi.mocked(api.previewRegistryPlugin).mockImplementation((id) => {
      const found = [ember, bsmanager, clock].find((u) => u.id === id);
      if (!found) return Promise.reject(new Error("unknown"));
      return Promise.resolve(previewOf(found, id === "bsmanager" ? REVIEW : NO_CHANGES));
    });
    await updateAll();
    // The confirmation lists the plugins that can go in one go, with their notes.
    expect(await screen.findByRole("heading", { name: "Update 2 plugins" })).toBeInTheDocument();
    expect(
      vi.mocked(api.previewRegistryPlugin).mock.calls.map(([id, version]) => `${id}@${version}`),
    ).toEqual(["bsmanager@0.2.0", "clock@2.0.1", "ember@1.1.0"]);
    const confirm = document.querySelectorAll("[data-plugin-confirm-row]");
    expect(Array.from(confirm).map((el) => el.getAttribute("data-plugin-confirm-row"))).toEqual([
      "clock",
      "ember",
    ]);
    expect(screen.getByText("Warmer reds.")).toBeInTheDocument();
    expect(screen.queryByText("Reads the new maps folder.")).toBeNull();
    expect(document.querySelector("[data-plugin-extension-warning]")).toBeNull();
    expect(api.installRegistryPlugin).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Back" })).toHaveFocus();

    vi.mocked(api.pluginUpdates).mockResolvedValue([bsmanager]);
    await fireEvent.click(screen.getByRole("button", { name: "Update 2 plugins" }));
    expect(await screen.findByRole("heading", { name: "Results" })).toBeInTheDocument();
    // No folders argument: the choices made before are kept.
    expect(vi.mocked(api.installRegistryPlugin).mock.calls).toEqual([
      ["clock", "2.0.1"],
      ["ember", "1.1.0"],
    ]);
    expect(screen.getByText("Updated: 2 · Failed: 0 · To review: 1")).toBeInTheDocument();
    expect(result("ember").getByText("Updated to 1.1.0")).toBeInTheDocument();
    expect(result("clock").getByText("Updated to 2.0.1")).toBeInTheDocument();
    expect(
      result("bsmanager").getByText(
        "Not updated: needs your review (asks for web access to api.example.com).",
      ),
    ).toBeInTheDocument();
    await waitFor(() => expect(toasts()).toContain("Updated 2 plugins."));
    // The update count was read again.
    await waitFor(() => expect(api.pluginUpdates).toHaveBeenCalledTimes(2));

    // Back to the list: only the plugin that needs a look is left, marked, with its own button.
    await fireEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(
      row("bsmanager").getByText("Needs your review: asks for web access to api.example.com."),
    ).toBeInTheDocument();
    expect(
      row("bsmanager").getByRole("button", { name: "Update BSManager to 0.2.0" }),
    ).toBeEnabled();
    expect(document.querySelector('[data-plugin-update-row="ember"]')).toBeNull();
  });

  it("names what an update asks for: folders, readable paths, an extension, a store", async () => {
    await show([bsmanager]);
    vi.mocked(api.previewRegistryPlugin).mockResolvedValue(
      previewOf(bsmanager, {
        newHttp: [],
        newRead: ["C:/games"],
        newFolders: [{ key: "inst", label: "Instances", hint: null, default: null }],
        addsExtension: true,
        addsStore: true,
        needsReview: true,
      }),
    );
    await updateAll();
    // Nothing could go in one go: no confirmation, a result for the plugin instead.
    expect(await screen.findByRole("heading", { name: "Results" })).toBeInTheDocument();
    expect(
      screen.getByText(
        "Not updated: needs your review (asks for read access to C:/games, a folder you choose: Instances, an extension (it runs inside the app), another store).",
      ),
    ).toBeInTheDocument();
    expect(api.installRegistryPlugin).not.toHaveBeenCalled();
  });

  it("shows the progress while it downloads", async () => {
    await show([ember, clock]);
    const second = deferred<RegistryPluginPreview>();
    vi.mocked(api.previewRegistryPlugin)
      .mockResolvedValueOnce(previewOf(clock))
      .mockReturnValueOnce(second.promise);
    await updateAll();
    expect(await screen.findByText("Downloading 2 of 2…")).toBeInTheDocument();
    // Nothing else can be started meanwhile, and Esc still reaches the dialog.
    expect(screen.getByRole("button", { name: "Update all" })).toBeDisabled();
    expect(screen.getByRole("dialog")).toHaveFocus();
    second.resolve(previewOf(ember));
    expect(await screen.findByRole("heading", { name: "Update 2 plugins" })).toBeInTheDocument();
  });

  it("warns about extensions in the confirmation", async () => {
    await show([ember, clock]);
    vi.mocked(api.previewRegistryPlugin).mockImplementation((id) => {
      const found = id === "ember" ? ember : clock;
      return Promise.resolve(previewOf(found, NO_CHANGES, { hasExtension: id === "ember" }));
    });
    await updateAll();
    const warning = await screen.findByText(/runs an extension/);
    expect(warning).toHaveTextContent("Ember Pack runs an extension");
    expect(warning).not.toHaveTextContent("Clock");
    expect(warning).toHaveTextContent("Only update plugins you trust");
  });

  it("Back from the confirmation installs nothing", async () => {
    await show([ember]);
    vi.mocked(api.previewRegistryPlugin).mockResolvedValue(previewOf(ember));
    await updateAll();
    await fireEvent.click(await screen.findByRole("button", { name: "Back" }));
    expect(api.installRegistryPlugin).not.toHaveBeenCalled();
    expect(screen.getByRole("list", { name: "Available updates" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Update all" })).toHaveFocus();
  });

  it("goes on after a failure and reports each plugin", async () => {
    await show([ember, bsmanager, clock]);
    vi.mocked(api.previewRegistryPlugin).mockImplementation((id) => {
      if (id === "bsmanager") return Promise.reject("The download broke off.");
      return Promise.resolve(previewOf(id === "ember" ? ember : clock));
    });
    vi.mocked(api.installRegistryPlugin).mockImplementation((id) =>
      id === "clock"
        ? Promise.reject('Could not back up the data of the plugin "clock".')
        : Promise.resolve(makePlugin({ id, name: id, providers: [] })),
    );
    await updateAll();
    await fireEvent.click(await screen.findByRole("button", { name: "Update 2 plugins" }));
    expect(await screen.findByRole("heading", { name: "Results" })).toBeInTheDocument();
    // Both were tried: the failed one did not stop the other.
    expect(api.installRegistryPlugin).toHaveBeenCalledTimes(2);
    expect(screen.getByText("Updated: 1 · Failed: 2 · To review: 0")).toBeInTheDocument();
    expect(result("bsmanager").getByText("Failed: The download broke off.")).toBeInTheDocument();
    expect(
      result("clock").getByText('Failed: Could not back up the data of the plugin "clock".'),
    ).toBeInTheDocument();
    expect(result("ember").getByText("Updated to 1.1.0")).toBeInTheDocument();
    await waitFor(() => expect(toasts()).toContain("Updated 1 plugin."));
  });

  it("rescans the stores of the updated plugins like a single update", async () => {
    await show([bsmanager]);
    vi.mocked(api.previewRegistryPlugin).mockResolvedValue(previewOf(bsmanager));
    await updateAll();
    await fireEvent.click(await screen.findByRole("button", { name: "Update 1 plugin" }));
    await waitFor(() => expect(api.scanLibrary).toHaveBeenCalledWith("bs-store"));
  });

  it("does not close while the updates are being installed", async () => {
    await show([ember]);
    vi.mocked(api.previewRegistryPlugin).mockResolvedValue(previewOf(ember));
    const install = deferred<ReturnType<typeof makePlugin>>();
    vi.mocked(api.installRegistryPlugin).mockReturnValue(install.promise);
    await updateAll();
    await fireEvent.click(await screen.findByRole("button", { name: "Update 1 plugin" }));
    expect(await screen.findByText("Installing 1 of 1…")).toBeInTheDocument();
    await fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(onClose).not.toHaveBeenCalled();
    install.resolve(makePlugin({ id: "ember", name: "Ember Pack", providers: [] }));
    expect(await screen.findByRole("heading", { name: "Results" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Close" })).toHaveFocus();
    await fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(onClose).toHaveBeenCalled();
  });
});
