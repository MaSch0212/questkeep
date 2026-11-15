import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { listen } from "@tauri-apps/api/event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../lib/api";
import { store } from "../lib/store";
import { makeGame, makeScript } from "../test-utils";
import { CommandBar } from "./CommandBar";
import { DetailsPanel } from "./DetailsPanel";
import { GameActionButtons } from "./GameActionButtons";
import { GameContextMenu } from "./GameContextMenu";

vi.mock("../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn<() => void>() }));

const game = makeGame({ id: "g1", name: "Cyberpunk 2077" });
const action = (id: string, name: string, icon: string | null = null) =>
  makeScript({
    id,
    name,
    kind: "action",
    icon,
    source: "program",
    program: "x.exe",
    runtime: null,
  });

const saves = action("a1", "Open save folder", "folder-open");
const backup = action("a2", "Back up saves", "save");
const tweaks = action("a3", "Open tweaks folder");

function give(...actions: ReturnType<typeof action>[]) {
  vi.mocked(api.gameScripts).mockResolvedValue({
    own: [],
    runGlobal: true,
    applied: [makeScript({ id: "p", kind: "pre" }), ...actions],
  });
}

beforeEach(async () => {
  vi.clearAllMocks();
  store.reset();
  vi.mocked(listen).mockResolvedValue(() => undefined);
  vi.mocked(api.runScript).mockResolvedValue(undefined);
  vi.mocked(api.launchProfiles).mockResolvedValue({ profiles: [], defaultId: null, playId: null });
  vi.mocked(api.listProviders).mockResolvedValue([]);
  vi.mocked(api.runningGames).mockResolvedValue([]);
  vi.mocked(api.listGames).mockResolvedValue([game]);
  vi.mocked(api.appInfo).mockResolvedValue({
    version: "1",
    dataDir: "",
    logsDir: "",
    portable: false,
    devTools: false,
  });
  await store.init();
});

describe("action buttons on the game page", () => {
  it("shows up to two actions as buttons and the rest under More scripts", async () => {
    give(saves, backup, tweaks);
    render(() => <GameActionButtons game={game} />);
    const group = await screen.findByRole("group", { name: "Actions" });
    expect(within(group).getByRole("button", { name: "Open save folder" })).toBeInTheDocument();
    expect(within(group).getByRole("button", { name: "Back up saves" })).toBeInTheDocument();
    expect(within(group).queryByRole("button", { name: "Open tweaks folder" })).toBeNull();

    await fireEvent.click(within(group).getByRole("button", { name: "Back up saves" }));
    expect(api.runScript).toHaveBeenCalledWith("a2", "g1");

    await fireEvent.click(within(group).getByRole("button", { name: "More scripts" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: "Open tweaks folder" }));
    expect(api.runScript).toHaveBeenCalledWith("a3", "g1");
  });

  it("has no More button for two actions and draws nothing for none", async () => {
    give(saves, backup);
    const { unmount } = render(() => <GameActionButtons game={game} />);
    await screen.findByRole("group", { name: "Actions" });
    expect(screen.queryByRole("button", { name: "More scripts" })).toBeNull();
    unmount();

    give();
    render(() => <GameActionButtons game={game} />);
    await waitFor(() => expect(api.gameScripts).toHaveBeenCalledTimes(2));
    expect(screen.queryByRole("group", { name: "Actions" })).toBeNull();
  });

  it("reloads when scripts change", async () => {
    give();
    render(() => <GameActionButtons game={game} />);
    await waitFor(() => expect(api.gameScripts).toHaveBeenCalled());
    give(saves);
    store.handleCoreEvent({ type: "scriptsChanged" });
    expect(await screen.findByRole("button", { name: "Open save folder" })).toBeInTheDocument();
  });

  it("appears in the game's details panel", async () => {
    give(saves);
    render(() => <DetailsPanel game={game} onClose={vi.fn<() => void>()} />);
    await fireEvent.click(await screen.findByRole("button", { name: "Open save folder" }));
    expect(api.runScript).toHaveBeenCalledWith("a1", "g1");
  });

  it("shows a failure to start as an error toast", async () => {
    give(saves);
    vi.mocked(api.runScript).mockRejectedValue("no such program");
    render(() => <GameActionButtons game={game} />);
    await fireEvent.click(await screen.findByRole("button", { name: "Open save folder" }));
    await waitFor(() =>
      expect(store.toasts().some((t) => t.message === "no such program")).toBe(true),
    );
  });
});

describe("Actions in the game's context menu", () => {
  it("offers an Actions submenu that runs the action", async () => {
    give(saves, backup);
    render(() => <GameContextMenu game={game} anchor={{ x: 0, y: 0 }} onDone={() => undefined} />);
    await fireEvent.click(await screen.findByRole("menuitem", { name: "Actions" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: "Back up saves" }));
    expect(api.runScript).toHaveBeenCalledWith("a2", "g1");
  });

  it("leaves the submenu out for a game without actions", async () => {
    give();
    render(() => <GameContextMenu game={game} anchor={{ x: 0, y: 0 }} onDone={() => undefined} />);
    await waitFor(() => expect(api.gameScripts).toHaveBeenCalled());
    expect(screen.queryByRole("menuitem", { name: "Actions" })).toBeNull();
  });
});

describe("Run: … in the command bar", () => {
  it("finds the selected game's actions and runs one", async () => {
    give(saves, backup);
    store.selectAndOpen("g1");
    render(() => <CommandBar />);
    const box = screen.getByRole("combobox", { name: "Find a game" });
    await fireEvent.focus(box);
    await waitFor(() => expect(api.gameScripts).toHaveBeenCalledWith("g1"));
    await fireEvent.input(box, { target: { value: "back up" } });
    const option = await screen.findByRole("option", { name: "Run: Back up saves" });
    await fireEvent.click(option);
    await waitFor(() => expect(api.runScript).toHaveBeenCalledWith("a2", "g1"));
  });
});
