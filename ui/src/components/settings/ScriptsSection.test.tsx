import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { GlobalScripts } from "../../bindings/GlobalScripts";
import * as api from "../../lib/api";
import { defaultQuery } from "../../lib/query";
import { store } from "../../lib/store";
import { makeGame, makeScript, RUNTIMES } from "../../test-utils";
import { ScriptsSection } from "./ScriptsSection";

vi.mock("../../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn<() => void>() }));

const eq = makeScript({ id: "a", name: "EQ: Gaming profile" });
const led = makeScript({
  id: "b",
  name: "LED wall",
  source: "linked",
  runtime: "python",
  program: "C:\\Tools\\show.py",
  timing: "onceRunning",
  filter: { ...defaultQuery(), installed: "yes" },
});
const onedrive = makeScript({
  id: "c",
  name: "Pause OneDrive",
  source: "program",
  program: "OneDrive.exe",
  args: "/shutdown",
  runtime: null,
  enabled: false,
});
const post = makeScript({ id: "p", name: "EQ: Default profile", kind: "post" });
const action = makeScript({
  id: "x",
  name: "Open save folder",
  kind: "action",
  source: "program",
  program: "explorer.exe",
  args: '"{InstallDir}"',
  runtime: null,
  icon: "folder-open",
});

function data(overrides: Partial<GlobalScripts> = {}): GlobalScripts {
  return {
    scripts: [eq, led, onedrive, post, action],
    preOrder: [
      { type: "script", id: "a" },
      { type: "script", id: "b" },
      { type: "gameScripts" },
      { type: "script", id: "c" },
    ],
    postOrder: [{ type: "gameScripts" }, { type: "script", id: "p" }],
    gamesWithPreScripts: 3,
    gamesWithPostScripts: 0,
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  store.reset();
  vi.mocked(api.globalScripts).mockResolvedValue(data());
  vi.mocked(api.scriptRuntimes).mockResolvedValue(RUNTIMES);
  vi.mocked(api.setScriptOrder).mockResolvedValue(undefined);
  vi.mocked(api.setScriptEnabled).mockResolvedValue(undefined);
  vi.mocked(api.deleteScript).mockResolvedValue(undefined);
  vi.mocked(api.saveScript).mockResolvedValue(eq);
  vi.mocked(api.runScript).mockResolvedValue(undefined);
  vi.mocked(api.listGames).mockResolvedValue([]);
});

const card = (name: string) => screen.getByRole("region", { name });
const rowOf = (name: string) => {
  const row = screen.getByText(name).closest("li");
  if (!row) throw new Error(name);
  return row;
};

describe("Settings › Scripts", () => {
  it("lists pre, post and action scripts with one-line summaries and the own-scripts rows", async () => {
    render(() => <ScriptsSection />);
    await screen.findByText("EQ: Gaming profile");
    const pre = card("Before a game starts");
    expect(
      within(pre).getByText("All games · Windows PowerShell 5.1 · inline script · before start"),
    ).toBeInTheDocument();
    expect(
      within(pre).getByText(/Python 3\.12 · C:\\Tools\\show\.py \(linked\) · once running/),
    ).toBeInTheDocument();
    expect(within(pre).getByText("Game's own pre scripts")).toBeInTheDocument();
    expect(
      within(pre).getByText("Set in each game's Edit dialog · 3 games have some"),
    ).toBeInTheDocument();
    // The order of the lists is the order of the rows.
    const names = within(pre)
      .getAllByRole("listitem")
      .map((li) => li.querySelector(".script-row__name")?.textContent);
    expect(names).toEqual([
      "EQ: Gaming profile",
      "LED wall",
      "Game's own pre scripts",
      "Pause OneDrive",
    ]);
    const postCard = card("After a game closes");
    expect(within(postCard).getByText("Game's own post scripts")).toBeInTheDocument();
    // Counted per kind: no game has own post scripts here.
    expect(within(postCard).getByText("Set in each game's Edit dialog")).toBeInTheDocument();
    expect(within(card("Actions")).getByText("Open save folder")).toBeInTheDocument();
    expect(within(pre).getByText("Filter")).toBeInTheDocument();
    expect(within(pre).getAllByText("Global")).toHaveLength(2);
    expect(within(pre).getByRole("switch", { name: "Enable Pause OneDrive" })).not.toBeChecked();
  });

  it("moves a row with Alt+Down and saves the order, keeping the games' placeholder", async () => {
    render(() => <ScriptsSection />);
    await screen.findByText("EQ: Gaming profile");
    await fireEvent.keyDown(rowOf("EQ: Gaming profile"), { key: "ArrowDown", altKey: true });
    expect(api.setScriptOrder).toHaveBeenCalledWith("pre", [
      { type: "script", id: "b" },
      { type: "script", id: "a" },
      { type: "gameScripts" },
      { type: "script", id: "c" },
    ]);
    // The list follows at once, before the core's event arrives.
    await waitFor(() => {
      const first = screen.getAllByRole("listitem")[0]?.querySelector(".script-row__name");
      expect(first?.textContent).toBe("LED wall");
    });
  });

  it("moves the placeholder row with Alt+Up and ignores a move past the end", async () => {
    render(() => <ScriptsSection />);
    await screen.findByText("EQ: Gaming profile");
    await fireEvent.keyDown(rowOf("EQ: Gaming profile"), { key: "ArrowUp", altKey: true });
    expect(api.setScriptOrder).not.toHaveBeenCalled();
    await fireEvent.keyDown(rowOf("Game's own pre scripts"), { key: "ArrowUp", altKey: true });
    expect(api.setScriptOrder).toHaveBeenCalledWith("pre", [
      { type: "script", id: "a" },
      { type: "gameScripts" },
      { type: "script", id: "b" },
      { type: "script", id: "c" },
    ]);
    // Plain arrows do nothing.
    vi.mocked(api.setScriptOrder).mockClear();
    await fireEvent.keyDown(rowOf("Pause OneDrive"), { key: "ArrowUp" });
    expect(api.setScriptOrder).not.toHaveBeenCalled();
  });

  it("orders actions through the same command", async () => {
    const second = makeScript({ id: "y", name: "Back up saves", kind: "action", position: 1n });
    vi.mocked(api.globalScripts).mockResolvedValue(
      data({ scripts: [{ ...action, position: 0n }, second] }),
    );
    render(() => <ScriptsSection />);
    await screen.findByText("Open save folder");
    await fireEvent.keyDown(rowOf("Open save folder"), { key: "ArrowDown", altKey: true });
    expect(api.setScriptOrder).toHaveBeenCalledWith("action", [
      { type: "script", id: "y" },
      { type: "script", id: "x" },
    ]);
  });

  it("moves a row from its ⋯ menu", async () => {
    render(() => <ScriptsSection />);
    await screen.findByText("EQ: Gaming profile");
    await fireEvent.click(screen.getByRole("button", { name: "More: EQ: Gaming profile" }));
    expect(screen.getByRole("menuitem", { name: "Move up" })).toBeDisabled();
    await fireEvent.click(screen.getByRole("menuitem", { name: "Move down" }));
    expect(api.setScriptOrder).toHaveBeenCalledWith("pre", [
      { type: "script", id: "b" },
      { type: "script", id: "a" },
      { type: "gameScripts" },
      { type: "script", id: "c" },
    ]);
  });

  it("enables and disables a script", async () => {
    render(() => <ScriptsSection />);
    await screen.findByText("Pause OneDrive");
    await fireEvent.click(screen.getByRole("switch", { name: "Enable Pause OneDrive" }));
    expect(api.setScriptEnabled).toHaveBeenCalledWith("c", true);
  });

  it("duplicates a script as a new one", async () => {
    render(() => <ScriptsSection />);
    await screen.findByText("EQ: Gaming profile");
    await fireEvent.click(screen.getByRole("button", { name: "More: EQ: Gaming profile" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: "Duplicate" }));
    expect(api.saveScript).toHaveBeenCalledWith(
      expect.objectContaining({ id: null, name: "EQ: Gaming profile (copy)", kind: "pre" }),
    );
  });

  it("deletes after confirming", async () => {
    render(() => <ScriptsSection />);
    await screen.findByText("EQ: Gaming profile");
    await fireEvent.click(screen.getByRole("button", { name: "More: EQ: Gaming profile" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: "Delete" }));
    expect(api.deleteScript).not.toHaveBeenCalled();
    const ask = screen.getByRole("dialog", { name: "Delete script" });
    await fireEvent.click(within(ask).getByRole("button", { name: "Delete" }));
    expect(api.deleteScript).toHaveBeenCalledWith("a");
  });

  it("runs a script now for a game chosen from a picker", async () => {
    const game = makeGame({ id: "g7", name: "Portal 2" });
    vi.mocked(api.listGames).mockResolvedValue([game]);
    render(() => <ScriptsSection />);
    await screen.findByText("Open save folder");
    await fireEvent.click(screen.getByRole("button", { name: "More: Open save folder" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: "Run now for a game…" }));
    await fireEvent.click(await screen.findByRole("option", { name: /Portal 2/ }));
    await waitFor(() => expect(api.runScript).toHaveBeenCalledWith("x", "g7"));
  });

  it("opens the dialog to add a script of the card's kind, and to edit one", async () => {
    render(() => <ScriptsSection />);
    await screen.findByText("EQ: Gaming profile");
    await fireEvent.click(screen.getByRole("button", { name: "Add action" }));
    const dialog = screen.getByRole("dialog", { name: "New action" });
    expect(within(dialog).getByRole("radio", { name: "As an action" })).toBeChecked();
    await fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await fireEvent.click(screen.getByRole("button", { name: "Edit: LED wall" }));
    expect(screen.getByRole("dialog", { name: "Edit script" })).toBeInTheDocument();
  });

  it("reloads when scripts change", async () => {
    render(() => <ScriptsSection />);
    await screen.findByText("EQ: Gaming profile");
    vi.mocked(api.globalScripts).mockResolvedValue(
      data({ scripts: [{ ...eq, name: "Renamed" }], preOrder: [{ type: "script", id: "a" }] }),
    );
    store.handleCoreEvent({ type: "scriptsChanged" });
    expect(await screen.findByText("Renamed")).toBeInTheDocument();
  });
});
