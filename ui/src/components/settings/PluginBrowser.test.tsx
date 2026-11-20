import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { openUrl } from "@tauri-apps/plugin-opener";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { PluginPreview } from "../../bindings/PluginPreview";
import type { RegistryListing } from "../../bindings/RegistryListing";
import type { RegistryPluginPreview } from "../../bindings/RegistryPluginPreview";
import type { RegistryPluginView } from "../../bindings/RegistryPluginView";
import * as api from "../../lib/api";
import { store } from "../../lib/store";
import { makePlugin, makeRegistryPlugin, makeVersion, NO_CHANGES } from "../../test-utils";
import { PluginBrowser } from "./PluginBrowser";

vi.mock("../../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));
vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn<() => void>(),
  openPath: vi.fn<() => void>(),
}));

const listing = (
  plugins: RegistryPluginView[],
  overrides: Partial<RegistryListing> = {},
): RegistryListing => ({
  offline: false,
  fetchedAt: "2026-11-20T12:00:00Z",
  plugins,
  ...overrides,
});

const theme = makeRegistryPlugin({
  id: "ember-pack",
  name: "Ember Pack",
  summary: "Warm themes.",
  author: "Ada",
  kinds: ["theme"],
  tags: ["warm"],
  versions: [makeVersion("1.0.0", { notes: "First release." }), makeVersion("1.1.0")],
});
const bsmanager = makeRegistryPlugin({
  id: "bsmanager",
  name: "BSManager",
  summary: "Beat Saber maps.",
  author: "Grace",
  kinds: ["store"],
  tags: ["beat-saber"],
  installed: { version: "0.9.0", pinned: false, fromRegistry: true },
  updateAvailable: true,
  compatibleVersions: ["1.0.0"],
  latest: "1.0.0",
  versions: [makeVersion("0.9.0"), makeVersion("1.0.0")],
});
const future = makeRegistryPlugin({
  id: "future",
  name: "Future Tool",
  summary: "Needs a newer app.",
  tags: [],
  author: "Zed",
  kinds: ["extension"],
  compatibleVersions: [],
  latest: null,
  incompatibleReason: "needs Questkeep 99.0.0",
  versions: [makeVersion("1.0.0", { compatible: false, minApp: "99.0.0" })],
});
const withYanked = makeRegistryPlugin({
  id: "yanky",
  name: "Yanky",
  summary: "Has a withdrawn version.",
  tags: [],
  author: "Yan",
  compatibleVersions: ["1.1.0"],
  latest: "1.1.0",
  versions: [makeVersion("1.0.0", { yanked: true }), makeVersion("1.1.0")],
});

const registryPreview = (
  overrides: Partial<RegistryPluginPreview> = {},
  preview: Partial<PluginPreview> = {},
): RegistryPluginPreview => ({
  preview: {
    id: "ember-pack",
    name: "Ember Pack",
    version: "1.1.0",
    description: "A set of warm themes.",
    author: "Ada",
    icon: null,
    stores: 0,
    hasExtension: false,
    themes: ["Ember"],
    permissions: { read: [], http: [], folders: [] },
    folders: [],
    installedVersion: null,
    ...preview,
  },
  released: "2026-11-20",
  notes: null,
  size: 2048,
  pinned: false,
  downgrade: false,
  changes: NO_CHANGES,
  ...overrides,
});

const pick = <T,>(list: readonly T[], index: number): T => {
  const item = list[index];
  if (item === undefined) throw new Error(`no item ${index}`);
  return item;
};
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
const toasts = () => store.toasts().map((t) => t.message);
const onClose = vi.fn<() => void>();

async function show(
  plugins = [theme, bsmanager, future, withYanked],
  extra: Partial<RegistryListing> = {},
) {
  vi.mocked(api.fetchPluginRegistry).mockResolvedValue(listing(plugins, extra));
  render(() => <PluginBrowser onClose={onClose} />);
  await screen.findByRole("list", { name: "Registry plugins" });
}

const card = (name: string) => screen.getByRole("button", { name: new RegExp(`^${name}`) });
const names = () =>
  within(screen.getByRole("list", { name: "Registry plugins" }))
    .getAllByRole("button")
    .map((b) => b.querySelector(".account__name")?.textContent);

beforeEach(() => {
  vi.clearAllMocks();
  store.reset();
  vi.mocked(api.pluginUpdates).mockResolvedValue([]);
  vi.mocked(api.scanLibrary).mockResolvedValue({ added: 0, updated: 0, uninstalled: 0 });
});

describe("PluginBrowser", () => {
  it("shows a loading state, then the plugins with updates first", async () => {
    vi.mocked(api.fetchPluginRegistry).mockResolvedValue(listing([theme, bsmanager]));
    render(() => <PluginBrowser onClose={onClose} />);
    expect(screen.getByText("Loading the plugin list…")).toBeInTheDocument();
    await screen.findByRole("list", { name: "Registry plugins" });
    expect(names()).toEqual(["BSManager", "Ember Pack"]);
    expect(card("BSManager")).toHaveTextContent("Update available");
    expect(card("BSManager")).toHaveTextContent("Installed v0.9.0");
    expect(card("BSManager")).toHaveTextContent("by Grace");
    expect(screen.getByRole("searchbox", { name: "Search plugins" })).toHaveFocus();
  });

  it("shows an error with Retry that loads again", async () => {
    vi.mocked(api.fetchPluginRegistry).mockRejectedValueOnce("No connection and no saved list.");
    render(() => <PluginBrowser onClose={onClose} />);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The plugin list could not be loaded: No connection and no saved list.",
    );
    vi.mocked(api.fetchPluginRegistry).mockResolvedValue(listing([theme]));
    await fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await screen.findByRole("list", { name: "Registry plugins" });
    expect(api.fetchPluginRegistry).toHaveBeenCalledTimes(2);
  });

  it("tells when the list comes from the cache", async () => {
    await show([theme], { offline: true, fetchedAt: "2026-11-19T08:00:00Z" });
    expect(screen.getByRole("status")).toHaveTextContent(/^Offline: showing the list from .*2026/);
  });

  it("says so when the registry is empty", async () => {
    vi.mocked(api.fetchPluginRegistry).mockResolvedValue(listing([]));
    render(() => <PluginBrowser onClose={onClose} />);
    expect(await screen.findByText("The registry lists no plugins yet.")).toBeInTheDocument();
  });

  it("searches name, summary, author and tags, and Esc clears the search first", async () => {
    await show();
    const search = screen.getByRole("searchbox", { name: "Search plugins" });
    await fireEvent.input(search, { target: { value: "beat-saber" } });
    expect(names()).toEqual(["BSManager"]);
    await fireEvent.input(search, { target: { value: "ember" } });
    expect(names()).toEqual(["Ember Pack"]);
    await fireEvent.input(search, { target: { value: "nothing like this" } });
    expect(screen.getByText("No plugin matches.")).toBeInTheDocument();
    await fireEvent.keyDown(search, { key: "Escape" });
    expect(search).toHaveValue("");
    expect(onClose).not.toHaveBeenCalled();
    expect(names()).toHaveLength(4);
  });

  it("filters by kind, installed and updates", async () => {
    await show();
    await fireEvent.click(screen.getByRole("button", { name: "Store" }));
    expect(screen.getByRole("button", { name: "Store" })).toHaveAttribute("aria-pressed", "true");
    expect(names()).toEqual(["BSManager"]);
    await fireEvent.click(screen.getByRole("button", { name: "Store" }));
    await fireEvent.click(screen.getByRole("button", { name: "Extension" }));
    expect(names()).toEqual(["Future Tool"]);
    await fireEvent.click(screen.getByRole("button", { name: "Extension" }));
    await fireEvent.click(screen.getByRole("button", { name: "Installed" }));
    expect(names()).toEqual(["BSManager"]);
    await fireEvent.click(screen.getByRole("button", { name: "Installed" }));
    await fireEvent.click(screen.getByRole("button", { name: "Updates" }));
    expect(names()).toEqual(["BSManager"]);
  });

  it("moves between the cards with the arrow keys and opens one with Enter or a click", async () => {
    await show();
    const search = screen.getByRole("searchbox", { name: "Search plugins" });
    await fireEvent.keyDown(search, { key: "ArrowDown" });
    expect(card("BSManager")).toHaveFocus();
    await fireEvent.keyDown(card("BSManager"), { key: "ArrowDown" });
    expect(card("Ember Pack")).toHaveFocus();
    await fireEvent.keyDown(card("Ember Pack"), { key: "End" });
    expect(card("Yanky")).toHaveFocus();
    await fireEvent.keyDown(card("Yanky"), { key: "Home" });
    expect(card("BSManager")).toHaveFocus();
    await fireEvent.keyDown(card("BSManager"), { key: "ArrowUp" });
    expect(search).toHaveFocus();
    // Enter on a focused card is the click of its button.
    await fireEvent.click(card("Ember Pack"));
    expect(await screen.findByRole("heading", { name: "Ember Pack" })).toBeInTheDocument();
  });

  it("shows the detail and goes back with Esc, handing the focus back to the card", async () => {
    await show();
    await fireEvent.click(card("Ember Pack"));
    expect(screen.getByRole("button", { name: "Back to the list" })).toHaveFocus();
    expect(screen.getByText("A set of warm themes.")).toBeInTheDocument();
    expect(screen.getByText("MIT")).toBeInTheDocument();
    await fireEvent.keyDown(screen.getByRole("button", { name: "Back to the list" }), {
      key: "Escape",
    });
    expect(onClose).not.toHaveBeenCalled();
    await waitFor(() => expect(card("Ember Pack")).toHaveFocus());
  });

  it("opens the repository in the system browser, not in the app", async () => {
    await show([{ ...theme, homepage: "https://ember.example" }]);
    await fireEvent.click(card("Ember Pack"));
    await fireEvent.click(
      screen.getByRole("button", { name: "https://github.com/ada/ember-pack" }),
    );
    expect(openUrl).toHaveBeenCalledWith("https://github.com/ada/ember-pack");
    await fireEvent.click(screen.getByRole("button", { name: "https://ember.example" }));
    expect(openUrl).toHaveBeenCalledWith("https://ember.example");
  });

  it("lists the installable versions newest first, each with an Install button", async () => {
    await show();
    await fireEvent.click(card("Ember Pack"));
    const versions = within(screen.getByRole("list", { name: "Versions" })).getAllByRole(
      "listitem",
    );
    expect(versions.map((v) => v.querySelector(".account__name")?.textContent)).toEqual([
      "v1.1.0",
      "v1.0.0",
    ]);
    expect(within(pick(versions, 1)).getByText("First release.")).toBeInTheDocument();
    expect(within(pick(versions, 0)).getByText(/2 KB/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Install Ember Pack 1.1.0" })).toBeEnabled();
  });

  it("downloads and checks, asks in the install dialog, installs and refreshes", async () => {
    await show([theme]);
    vi.mocked(api.previewRegistryPlugin).mockResolvedValue(
      registryPreview({ notes: "Warmer reds." }),
    );
    vi.mocked(api.installRegistryPlugin).mockResolvedValue(
      makePlugin({ id: "ember-pack", name: "Ember Pack", providers: [] }),
    );
    await fireEvent.click(card("Ember Pack"));
    const installed = {
      ...theme,
      installed: { version: "1.1.0", pinned: false, fromRegistry: true },
    };
    await fireEvent.click(screen.getByRole("button", { name: "Install Ember Pack 1.1.0" }));
    expect(api.previewRegistryPlugin).toHaveBeenCalledWith("ember-pack", "1.1.0");
    const dialog = await screen.findByRole("dialog", { name: "Install this plugin?" });
    expect(within(dialog).getByText(/Plugins are third-party code/)).toBeInTheDocument();
    expect(within(dialog).getByText("Warmer reds.")).toBeInTheDocument();
    expect(within(dialog).queryByText(/backed up first/)).toBeNull();
    expect(within(dialog).queryByText(/keeps it from being updated/)).toBeNull();
    expect(api.installRegistryPlugin).not.toHaveBeenCalled();
    vi.mocked(api.fetchPluginRegistry).mockResolvedValue(listing([installed]));
    await fireEvent.click(within(dialog).getByRole("button", { name: "Install" }));
    await waitFor(() =>
      expect(api.installRegistryPlugin).toHaveBeenCalledWith("ember-pack", "1.1.0"),
    );
    await waitFor(() => expect(toasts()).toContain("Installed Ember Pack v1.1.0."));
    // The card's state follows: the current version is marked, and it can be reinstalled.
    expect(
      await screen.findByRole("button", { name: "Reinstall Ember Pack 1.1.0" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Downgrade Ember Pack 1.0.0" })).toBeInTheDocument();
  });

  it("shows a busy state while the zip is downloaded", async () => {
    await show([theme]);
    const download = deferred<RegistryPluginPreview>();
    vi.mocked(api.previewRegistryPlugin).mockReturnValue(download.promise);
    await fireEvent.click(card("Ember Pack"));
    await fireEvent.click(screen.getByRole("button", { name: "Install Ember Pack 1.1.0" }));
    const busy = screen.getByRole("button", { name: "Install Ember Pack 1.1.0" });
    expect(busy).toHaveTextContent("Downloading…");
    expect(busy).toHaveAttribute("aria-busy", "true");
    expect(screen.getByRole("button", { name: "Install Ember Pack 1.0.0" })).toBeDisabled();
    download.resolve(registryPreview());
    await screen.findByRole("dialog", { name: "Install this plugin?" });
  });

  it("reports a failed download and installs nothing", async () => {
    await show([theme]);
    vi.mocked(api.previewRegistryPlugin).mockRejectedValue("The download does not match its hash.");
    await fireEvent.click(card("Ember Pack"));
    await fireEvent.click(screen.getByRole("button", { name: "Install Ember Pack 1.1.0" }));
    await waitFor(() => expect(toasts()).toContain("The download does not match its hash."));
    expect(screen.queryByRole("dialog", { name: "Install this plugin?" })).toBeNull();
    expect(screen.getByRole("button", { name: "Install Ember Pack 1.1.0" })).toBeEnabled();
  });

  it("offers Update for newer, Downgrade for older versions, and explains the pin and backup", async () => {
    await show([bsmanager]);
    await fireEvent.click(card("BSManager"));
    expect(screen.getByRole("button", { name: "Update BSManager 1.0.0" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Reinstall BSManager 0.9.0" })).toBeInTheDocument();
    vi.mocked(api.previewRegistryPlugin).mockResolvedValue(
      registryPreview({ downgrade: true, pinned: true }, { installedVersion: "1.0.0" }),
    );
    await fireEvent.click(screen.getByRole("button", { name: "Reinstall BSManager 0.9.0" }));
    const dialog = await screen.findByRole("dialog", { name: "Install this plugin?" });
    expect(
      within(dialog).getByText("Your data for this plugin is backed up first."),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByText(
        "Installing this version keeps it from being updated automatically.",
      ),
    ).toBeInTheDocument();
  });

  it("explains a pinned plugin in its page", async () => {
    await show([
      {
        ...bsmanager,
        updateAvailable: false,
        installed: { version: "0.9.0", pinned: true, fromRegistry: true },
      },
    ]);
    expect(card("BSManager")).toHaveTextContent("Pinned");
    await fireEvent.click(card("BSManager"));
    expect(
      screen.getByText(/Pinned to v0.9.0: this plugin is not updated automatically/),
    ).toBeVisible();
  });

  it("cancelling the install dialog installs nothing", async () => {
    await show([theme]);
    vi.mocked(api.previewRegistryPlugin).mockResolvedValue(registryPreview());
    await fireEvent.click(card("Ember Pack"));
    await fireEvent.click(screen.getByRole("button", { name: "Install Ember Pack 1.1.0" }));
    const dialog = await screen.findByRole("dialog", { name: "Install this plugin?" });
    await fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(api.installRegistryPlugin).not.toHaveBeenCalled();
    // Esc in the dialog closes only the dialog, not the browser.
    expect(onClose).not.toHaveBeenCalled();
  });

  it("marks a plugin that no version of runs here, with a disabled Install", async () => {
    await show();
    expect(card("Future Tool")).toHaveTextContent("Not compatible: needs Questkeep 99.0.0");
    await fireEvent.click(card("Future Tool"));
    expect(screen.getByRole("alert")).toHaveTextContent("Not compatible: needs Questkeep 99.0.0");
    expect(screen.getByRole("button", { name: "Install" })).toBeDisabled();
    await fireEvent.click(screen.getByLabelText("Show all versions"));
    expect(
      screen.getByText("Needs Questkeep 99.0.0 or newer and plugin API 1"),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Install Future Tool 1.0.0" })).toBeDisabled();
  });

  it("hides withdrawn versions until all versions are shown, then greys them out", async () => {
    await show([withYanked]);
    await fireEvent.click(card("Yanky"));
    expect(screen.queryByText("Withdrawn by the author")).toBeNull();
    expect(screen.queryByRole("button", { name: "Install Yanky 1.0.0" })).toBeNull();
    await fireEvent.click(screen.getByLabelText("Show all versions"));
    expect(screen.getByText("Withdrawn by the author")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Install Yanky 1.0.0" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Install Yanky 1.1.0" })).toBeEnabled();
    await fireEvent.click(screen.getByRole("button", { name: "Install Yanky 1.0.0" }));
    expect(api.previewRegistryPlugin).not.toHaveBeenCalled();
  });

  it("closes with Esc from the list", async () => {
    await show([theme]);
    await fireEvent.keyDown(card("Ember Pack"), { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
