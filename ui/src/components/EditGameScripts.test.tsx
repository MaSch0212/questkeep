import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { GameScripts } from "../bindings/GameScripts";
import * as api from "../lib/api";
import { defaultQuery } from "../lib/query";
import { store } from "../lib/store";
import { makeGame, makeScript, RUNTIMES, SCRIPT_VARIABLES } from "../test-utils";
import { EditGameDialog } from "./EditGameDialog";

vi.mock("../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn<() => void>() }));

const emptyMeta = () => ({
  description: null,
  releaseDate: null,
  developers: [],
  publishers: [],
  genres: [],
  tags: [],
  features: [],
  series: [],
  ageRatings: [],
  platforms: [],
  gameModes: [],
  themes: [],
  perspectives: [],
  criticScore: null,
  communityScore: null,
  installSize: null,
  timeToBeat: null,
  links: [],
});

const game = makeGame({ id: "g1", name: "Cyberpunk 2077" });
const mods = makeScript({
  id: "o1",
  name: "Disable mods overlay",
  kind: "pre",
  gameId: "g1",
  timing: "onceRunning",
  runtime: "windowsPowershell",
  position: 0n,
});
const folder = makeScript({
  id: "o2",
  name: "Open plugins folder",
  kind: "action",
  gameId: "g1",
  source: "program",
  program: "explorer.exe",
  args: '"{InstallDir}\\plugins"',
  runtime: null,
  position: 1n,
});
const eq = makeScript({ id: "gl", name: "EQ: Gaming profile", kind: "pre" });
const eqFiltered = makeScript({
  id: "gf",
  name: "Racing lights",
  kind: "pre",
  filter: { ...defaultQuery(), installed: "yes" },
});
const backup = makeScript({ id: "bk", name: "Back up saves", kind: "action" });

function scripts(overrides: Partial<GameScripts> = {}): GameScripts {
  return {
    own: [mods, folder],
    runGlobal: true,
    applied: [eq, mods, eqFiltered, folder, backup],
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  store.reset();
  vi.mocked(api.gameEditInfo).mockResolvedValue({
    edits: { meta: emptyMeta(), edited: [], hiddenLinks: [] },
    automatic: emptyMeta(),
    automaticSources: {},
    links: [],
    automaticSortingName: "Cyberpunk 2077",
    sortingName: null,
    userImages: { cover: false, background: false, icon: false },
    metadataLink: null,
  });
  vi.mocked(api.launchProfiles).mockResolvedValue({ profiles: [], defaultId: null, playId: null });
  vi.mocked(api.gameScripts).mockResolvedValue(scripts());
  vi.mocked(api.scriptRuntimes).mockResolvedValue(RUNTIMES);
  vi.mocked(api.scriptVariables).mockResolvedValue(SCRIPT_VARIABLES);
  vi.mocked(api.setRunGlobalScripts).mockResolvedValue(undefined);
  vi.mocked(api.setScriptEnabled).mockResolvedValue(undefined);
  vi.mocked(api.setOwnScriptOrder).mockResolvedValue(undefined);
  vi.mocked(api.deleteScript).mockResolvedValue(undefined);
  vi.mocked(api.saveScript).mockResolvedValue(mods);
  vi.mocked(api.listGames).mockResolvedValue([]);
  vi.mocked(api.updateGame).mockResolvedValue(game);
  vi.mocked(api.getGame).mockResolvedValue(game);
});

const dialog = () => screen.getByRole("dialog", { name: "Edit game" });

async function openTab(onClose: () => void = () => undefined) {
  render(() => <EditGameDialog game={game} section="scripts" onClose={onClose} />);
  await within(dialog()).findByRole("heading", { name: "This game's scripts" });
  await within(dialog()).findByRole("list", { name: "This game's scripts" });
}

const own = () => within(dialog()).getByRole("list", { name: "This game's scripts" });

describe("Edit game dialog: Scripts tab", () => {
  it("lists the game's own scripts with their kind, the opt-out and what applies in run order", async () => {
    await openTab();
    const tab = within(dialog());
    expect(tab.getByRole("button", { name: /^Scripts\s*2$/ })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(
      tab.getByText("Pre · Windows PowerShell 5.1 · inline script · once running"),
    ).toBeInTheDocument();
    expect(tab.getByText('Action · explorer.exe "{InstallDir}\\plugins"')).toBeInTheDocument();
    expect(tab.getByRole("checkbox", { name: "Run global scripts for this game" })).toBeChecked();

    const applies = tab.getByRole("list", { name: "Scripts that apply" });
    const lines = within(applies)
      .getAllByRole("listitem")
      .map((li) => li.textContent);
    // The game's own pre script runs at the placeholder: between the global and filtered ones.
    expect(lines).toEqual([
      "PreEQ: Gaming profile",
      "PreThis game's scriptsDisable mods overlay",
      "PreRacing lights· filter matches",
      "ActionThis game's scriptsOpen plugins folder",
      "ActionBack up saves",
    ]);
  });

  it("saves the opt-out at once", async () => {
    await openTab();
    await fireEvent.click(
      within(dialog()).getByRole("checkbox", { name: "Run global scripts for this game" }),
    );
    expect(api.setRunGlobalScripts).toHaveBeenCalledWith("g1", false);
  });

  it("adds a script of the game: the dialog has no scope, and it saves with the game", async () => {
    await openTab();
    await fireEvent.click(within(dialog()).getByRole("button", { name: "Add script" }));
    const script = screen.getByRole("dialog", { name: "New script" });
    expect(within(script).queryByRole("radio", { name: "All games" })).toBeNull();
    await fireEvent.input(within(script).getByRole("textbox", { name: "Name" }), {
      target: { value: "Mine" },
    });
    await fireEvent.input(within(script).getByRole("textbox", { name: "Program" }), {
      target: { value: "a.exe" },
    });
    await fireEvent.click(within(script).getByRole("button", { name: "Add" }));
    await waitFor(() =>
      expect(api.saveScript).toHaveBeenCalledWith(
        expect.objectContaining({ id: null, gameId: "g1", name: "Mine", filter: null }),
      ),
    );
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "New script" })).toBeNull());
  });

  it("enables, reorders and deletes own scripts", async () => {
    await openTab();
    const tab = within(dialog());
    await fireEvent.click(tab.getByRole("switch", { name: "Enable Disable mods overlay" }));
    expect(api.setScriptEnabled).toHaveBeenCalledWith("o1", false);

    const row = within(own()).getByText("Disable mods overlay").closest("li") as HTMLElement;
    await fireEvent.keyDown(row, { key: "ArrowDown", altKey: true });
    expect(api.setOwnScriptOrder).toHaveBeenCalledWith(["o2", "o1"]);

    await fireEvent.click(tab.getByRole("button", { name: "More: Open plugins folder" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: "Delete" }));
    await fireEvent.click(
      within(screen.getByRole("dialog", { name: "Delete script" })).getByRole("button", {
        name: "Delete",
      }),
    );
    expect(api.deleteScript).toHaveBeenCalledWith("o2");
  });

  it("opens Settings › Scripts at once when nothing is unsaved", async () => {
    const onClose = vi.fn<() => void>();
    await openTab(onClose);
    await fireEvent.click(within(dialog()).getByRole("button", { name: "Open in Settings" }));
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(store.page()).toBe("settings");
    expect(store.takeSettingsSection()).toBe("scripts");
    expect(screen.queryByRole("dialog", { name: "Unsaved changes" })).toBeNull();
  });

  it("asks first when the dialog has unsaved changes: Cancel keeps it open", async () => {
    const onClose = vi.fn<() => void>();
    await openTab(onClose);
    await fireEvent.click(within(dialog()).getByRole("button", { name: "General" }));
    await fireEvent.input(within(dialog()).getByRole("textbox", { name: "Name" }), {
      target: { value: "Cyberpunk 2078" },
    });
    await fireEvent.click(within(dialog()).getByRole("button", { name: /^Scripts/ }));
    await fireEvent.click(within(dialog()).getByRole("button", { name: "Open in Settings" }));
    const ask = screen.getByRole("dialog", { name: "Unsaved changes" });
    expect(onClose).not.toHaveBeenCalled();
    await fireEvent.click(within(ask).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog", { name: "Unsaved changes" })).toBeNull();
    expect(onClose).not.toHaveBeenCalled();
    expect(store.page()).not.toBe("settings");
  });

  it("Discard and open drops the changes", async () => {
    const onClose = vi.fn<() => void>();
    await openTab(onClose);
    await fireEvent.click(within(dialog()).getByRole("button", { name: "General" }));
    await fireEvent.input(within(dialog()).getByRole("textbox", { name: "Name" }), {
      target: { value: "Cyberpunk 2078" },
    });
    await fireEvent.click(within(dialog()).getByRole("button", { name: /^Scripts/ }));
    await fireEvent.click(within(dialog()).getByRole("button", { name: "Open in Settings" }));
    await fireEvent.click(screen.getByRole("button", { name: "Discard and open" }));
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(api.updateGame).not.toHaveBeenCalled();
    expect(store.page()).toBe("settings");
  });

  it("Save and open saves, then opens Settings", async () => {
    const onClose = vi.fn<() => void>();
    await openTab(onClose);
    await fireEvent.click(within(dialog()).getByRole("button", { name: "General" }));
    await fireEvent.input(within(dialog()).getByRole("textbox", { name: "Name" }), {
      target: { value: "Cyberpunk 2078" },
    });
    await fireEvent.click(within(dialog()).getByRole("button", { name: /^Scripts/ }));
    await fireEvent.click(within(dialog()).getByRole("button", { name: "Open in Settings" }));
    await fireEvent.click(screen.getByRole("button", { name: "Save and open" }));
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
    expect(api.updateGame).toHaveBeenCalledWith(
      "g1",
      expect.objectContaining({ name: "Cyberpunk 2078" }),
    );
    expect(store.page()).toBe("settings");
  });
});
