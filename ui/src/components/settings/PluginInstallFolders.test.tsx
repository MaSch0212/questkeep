import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { open } from "@tauri-apps/plugin-dialog";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../../lib/api";
import { store } from "../../lib/store";
import {
  makeFolder,
  makePlugin,
  makePreview,
  makeRegistryPlugin,
  makeVersion,
  NO_CHANGES,
} from "../../test-utils";
import { PluginBrowser } from "./PluginBrowser";
import { PluginsSection } from "./PluginsSection";

vi.mock("../../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn<() => void>() }));
vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn<() => void>(),
  openPath: vi.fn<() => void>(),
}));

const DEFAULT = "C:\\Users\\me\\BSManager\\BSInstances";
const FIXTURE = "D:\\BSManager\\BSInstances";

const instances = (overrides = {}) => makeFolder(overrides);
const apps = (overrides = {}) =>
  makeFolder({
    key: "app",
    label: "BSManager app folder",
    hint: null,
    mount: "/app",
    default: null,
    defaultPath: null,
    effectivePath: null,
    ...overrides,
  });
const storePreview = (...folders: ReturnType<typeof makeFolder>[]) =>
  makePreview({
    id: "bs",
    name: "BSManager",
    permissions: {
      read: [],
      http: [],
      filesystem: false,
      registry: false,
      launcher: [],
      login: [],
      folders: folders.map(({ key, label, hint, default: d }) => ({
        key,
        label,
        hint,
        default: d,
      })),
    },
    folders,
  });
/** The plugin as the install returns it. */
const installed = (...folders: ReturnType<typeof makeFolder>[]) =>
  makePlugin({ id: "bs", name: "BSManager", providers: ["bs-store"], folders });
const chosenInstances = instances({
  path: FIXTURE,
  effectivePath: FIXTURE,
  exists: true,
});

const toasts = () => store.toasts().map((t) => ({ kind: t.kind, message: t.message }));
const messages = () => toasts().map((t) => t.message);

async function openFolderDialog(preview = storePreview(instances(), apps())) {
  vi.mocked(api.listPlugins).mockResolvedValue([]);
  await store.loadPlugins();
  render(() => <PluginsSection />);
  vi.mocked(open).mockResolvedValueOnce("C:\\Dev\\bsmanager");
  vi.mocked(api.previewPlugin).mockResolvedValue(preview);
  await fireEvent.click(screen.getByRole("button", { name: "Install from folder…" }));
  const dialog = await screen.findByRole("dialog", { name: "Install this plugin?" });
  return within(dialog);
}

beforeEach(() => {
  vi.clearAllMocks();
  store.reset();
  vi.mocked(api.scanLibrary).mockResolvedValue({ added: 2, updated: 0, uninstalled: 0 });
  vi.mocked(api.pluginUpdates).mockResolvedValue([]);
  vi.mocked(api.pluginFileUrl).mockImplementation((id, path) => `qk://plugins/${id}/${path}`);
});

describe("choosing folders while installing", () => {
  it("lists each folder with its default and says when it is not there", async () => {
    const dialog = await openFolderDialog();
    const first = within(dialog.getByRole("group", { name: "BSManager instances folder" }));
    expect(first.getByText("The folder that contains your Beat Saber instances.")).toBeVisible();
    expect(first.getByText(`Default: ${DEFAULT}`)).toBeVisible();
    expect(first.getByRole("alert")).toHaveTextContent("not found");
    expect(first.getByText(/install anyway and choose this folder later/)).toBeVisible();
    expect(first.getByRole("button", { name: /Choose the folder for/ })).toBeEnabled();
    expect(first.queryByRole("button", { name: /Use the default folder/ })).toBeNull();
    const second = within(dialog.getByRole("group", { name: "BSManager app folder" }));
    expect(second.getByText("Not set")).toBeVisible();
    expect(second.getByText(/install anyway/)).toBeVisible();
    // The permission text no longer says the user decides later.
    expect(dialog.getAllByText(/May read the folder you choose/)).toHaveLength(2);
  });

  it("shows a folder whose default exists without a warning", async () => {
    const dialog = await openFolderDialog(
      storePreview(instances({ defaultExists: true, exists: true })),
    );
    const row = within(dialog.getByRole("group", { name: "BSManager instances folder" }));
    expect(row.queryByRole("alert")).toBeNull();
    expect(row.queryByText(/install anyway/)).toBeNull();
  });

  it("choosing and resetting a folder changes the row", async () => {
    const dialog = await openFolderDialog();
    vi.mocked(open).mockResolvedValueOnce(FIXTURE);
    await fireEvent.click(
      dialog.getByRole("button", { name: "Choose the folder for BSManager instances folder" }),
    );
    expect(open).toHaveBeenLastCalledWith(
      expect.objectContaining({
        directory: true,
        title: "Choose the folder: BSManager instances folder",
      }),
    );
    const row = within(await dialog.findByRole("group", { name: "BSManager instances folder" }));
    expect(await row.findByText(FIXTURE)).toBeVisible();
    expect(row.queryByRole("alert")).toBeNull();
    expect(row.queryByText(/install anyway/)).toBeNull();
    await fireEvent.click(
      row.getByRole("button", { name: "Use the default folder for BSManager instances folder" }),
    );
    expect(row.getByText(`Default: ${DEFAULT}`)).toBeVisible();
    expect(row.getByRole("alert")).toHaveTextContent("not found");
    expect(row.queryByRole("button", { name: /Use the default folder/ })).toBeNull();
  });

  it("starts the picker in the folder that is shown, if it exists", async () => {
    const dialog = await openFolderDialog(
      storePreview(instances({ defaultExists: true, exists: true })),
    );
    vi.mocked(open).mockResolvedValueOnce(null);
    await fireEvent.click(dialog.getByRole("button", { name: /Choose the folder for/ }));
    await waitFor(() =>
      expect(open).toHaveBeenLastCalledWith(expect.objectContaining({ defaultPath: DEFAULT })),
    );
  });

  it("passes the chosen folders to the install command and scans", async () => {
    const dialog = await openFolderDialog();
    vi.mocked(open).mockResolvedValueOnce(FIXTURE);
    await fireEvent.click(
      dialog.getByRole("button", { name: "Choose the folder for BSManager instances folder" }),
    );
    await dialog.findByText(FIXTURE);
    vi.mocked(api.installPlugin).mockResolvedValue(installed(chosenInstances, apps()));
    await fireEvent.click(dialog.getByRole("button", { name: "Install" }));
    await waitFor(() =>
      expect(api.installPlugin).toHaveBeenCalledWith("C:\\Dev\\bsmanager", {
        instances: FIXTURE,
      }),
    );
    // The app folder is still unset and has no default: no scan, the hint instead.
    await waitFor(() =>
      expect(messages().some((m) => m.includes("BSManager app folder in"))).toBe(true),
    );
    expect(api.scanLibrary).not.toHaveBeenCalled();
  });

  it("scans when every folder is fine", async () => {
    const dialog = await openFolderDialog(storePreview(instances()));
    vi.mocked(open).mockResolvedValueOnce(FIXTURE);
    await fireEvent.click(dialog.getByRole("button", { name: /Choose the folder for/ }));
    await dialog.findByText(FIXTURE);
    vi.mocked(api.installPlugin).mockResolvedValue(installed(chosenInstances));
    await fireEvent.click(dialog.getByRole("button", { name: "Install" }));
    await waitFor(() => expect(api.scanLibrary).toHaveBeenCalledWith("bs-store"));
    expect(api.installPlugin).toHaveBeenCalledWith("C:\\Dev\\bsmanager", { instances: FIXTURE });
    expect(toasts().filter((t) => t.kind === "error")).toEqual([]);
    expect(messages().some((m) => m.includes("Settings > Plugins"))).toBe(false);
  });

  it("installs without folders when nothing was chosen, and says where to choose instead of scanning", async () => {
    const dialog = await openFolderDialog(storePreview(instances(), apps()));
    vi.mocked(api.installPlugin).mockResolvedValue(installed(instances(), apps()));
    await fireEvent.click(dialog.getByRole("button", { name: "Install" }));
    await waitFor(() => expect(api.installPlugin).toHaveBeenCalledWith("C:\\Dev\\bsmanager"));
    await waitFor(() =>
      expect(messages()).toContain(
        "Choose BSManager instances folder, BSManager app folder in Settings > Plugins > BSManager > Folders to list its games.",
      ),
    );
    expect(api.scanLibrary).not.toHaveBeenCalled();
    expect(toasts().filter((t) => t.kind === "error")).toEqual([]);
    expect(messages()).toContain("Installed BSManager.");
  });

  it("scans when the default folder exists and nothing was chosen", async () => {
    const fine = instances({ defaultExists: true, exists: true });
    const dialog = await openFolderDialog(storePreview(fine));
    vi.mocked(api.installPlugin).mockResolvedValue(installed(fine));
    await fireEvent.click(dialog.getByRole("button", { name: "Install" }));
    await waitFor(() => expect(api.scanLibrary).toHaveBeenCalledWith("bs-store"));
  });

  it("an update preselects the current choice and only sends changes", async () => {
    const current = instances({
      path: FIXTURE,
      effectivePath: FIXTURE,
      exists: true,
    });
    const dialog = await openFolderDialog({
      ...storePreview(current),
      installedVersion: "1.0.0",
    });
    const row = within(dialog.getByRole("group", { name: "BSManager instances folder" }));
    expect(row.getByText(FIXTURE)).toBeVisible();
    expect(row.getByRole("button", { name: /Use the default folder/ })).toBeEnabled();
    vi.mocked(api.installPlugin).mockResolvedValue(installed(current));
    await fireEvent.click(dialog.getByRole("button", { name: "Install" }));
    // Keeping the choice sends nothing: the installed one stays.
    await waitFor(() => expect(api.installPlugin).toHaveBeenCalledWith("C:\\Dev\\bsmanager"));
    await waitFor(() => expect(api.scanLibrary).toHaveBeenCalled());
  });

  it("resetting an installed choice sends a blank path", async () => {
    const current = instances({ path: FIXTURE, effectivePath: FIXTURE, exists: true });
    const dialog = await openFolderDialog({ ...storePreview(current), installedVersion: "1.0.0" });
    await fireEvent.click(
      dialog.getByRole("button", { name: "Use the default folder for BSManager instances folder" }),
    );
    vi.mocked(api.installPlugin).mockResolvedValue(installed(instances()));
    await fireEvent.click(dialog.getByRole("button", { name: "Install" }));
    await waitFor(() =>
      expect(api.installPlugin).toHaveBeenCalledWith("C:\\Dev\\bsmanager", { instances: "" }),
    );
  });

  it("shows why the install was refused", async () => {
    const dialog = await openFolderDialog();
    vi.mocked(api.installPlugin).mockRejectedValue("The folder D:\\Nope does not exist.");
    await fireEvent.click(dialog.getByRole("button", { name: "Install" }));
    await waitFor(() => expect(messages()).toContain("The folder D:\\Nope does not exist."));
    expect(toasts().filter((t) => t.kind === "error")).toHaveLength(1);
    expect(api.scanLibrary).not.toHaveBeenCalled();
  });

  it("a cancelled picker changes nothing", async () => {
    const dialog = await openFolderDialog();
    vi.mocked(open).mockResolvedValueOnce(null);
    await fireEvent.click(
      dialog.getByRole("button", { name: "Choose the folder for BSManager instances folder" }),
    );
    await waitFor(() => expect(open).toHaveBeenCalledTimes(2));
    expect(dialog.getByText(`Default: ${DEFAULT}`)).toBeVisible();
  });

  it("a plugin without folders has no folder block", async () => {
    const dialog = await openFolderDialog(makePreview());
    expect(dialog.queryByText("Choose the folders")).toBeNull();
  });
});

describe("choosing folders in the registry's install dialog", () => {
  const entry = makeRegistryPlugin({
    id: "bs",
    name: "BSManager",
    summary: "Beat Saber maps.",
    kinds: ["store"],
    tags: [],
    author: "Grace",
    compatibleVersions: ["1.0.0"],
    latest: "1.0.0",
    versions: [makeVersion("1.0.0")],
  });

  it("passes the chosen folders to the registry install", async () => {
    vi.mocked(api.fetchPluginRegistry).mockResolvedValue({
      offline: false,
      fetchedAt: "2026-11-20T12:00:00Z",
      plugins: [entry],
    });
    vi.mocked(api.previewRegistryPlugin).mockResolvedValue({
      preview: storePreview(instances()),
      released: null,
      notes: null,
      size: 1024,
      pinned: false,
      downgrade: false,
      changes: NO_CHANGES,
    });
    vi.mocked(api.installRegistryPlugin).mockResolvedValue(installed(chosenInstances));
    render(() => <PluginBrowser onClose={() => undefined} />);
    await fireEvent.click(await screen.findByRole("button", { name: /^BSManager/ }));
    await fireEvent.click(await screen.findByRole("button", { name: "Install BSManager 1.0.0" }));
    const dialog = within(await screen.findByRole("dialog", { name: "Install this plugin?" }));
    vi.mocked(open).mockResolvedValueOnce(FIXTURE);
    await fireEvent.click(dialog.getByRole("button", { name: /Choose the folder for/ }));
    await dialog.findByText(FIXTURE);
    await fireEvent.click(dialog.getByRole("button", { name: "Install" }));
    await waitFor(() =>
      expect(api.installRegistryPlugin).toHaveBeenCalledWith("bs", "1.0.0", { instances: FIXTURE }),
    );
    await waitFor(() => expect(api.scanLibrary).toHaveBeenCalledWith("bs-store"));
  });

  it("says where to choose instead of scanning when the folder is still missing", async () => {
    vi.mocked(api.fetchPluginRegistry).mockResolvedValue({
      offline: false,
      fetchedAt: "2026-11-20T12:00:00Z",
      plugins: [entry],
    });
    vi.mocked(api.previewRegistryPlugin).mockResolvedValue({
      preview: storePreview(instances()),
      released: null,
      notes: null,
      size: 1024,
      pinned: false,
      downgrade: false,
      changes: NO_CHANGES,
    });
    vi.mocked(api.installRegistryPlugin).mockResolvedValue(installed(instances()));
    render(() => <PluginBrowser onClose={() => undefined} />);
    await fireEvent.click(await screen.findByRole("button", { name: /^BSManager/ }));
    await fireEvent.click(await screen.findByRole("button", { name: "Install BSManager 1.0.0" }));
    const dialog = within(await screen.findByRole("dialog", { name: "Install this plugin?" }));
    await fireEvent.click(dialog.getByRole("button", { name: "Install" }));
    await waitFor(() =>
      expect(messages()).toContain(
        "Choose BSManager instances folder in Settings > Plugins > BSManager > Folders to list its games.",
      ),
    );
    expect(api.installRegistryPlugin).toHaveBeenCalledWith("bs", "1.0.0");
    expect(api.scanLibrary).not.toHaveBeenCalled();
  });
});
