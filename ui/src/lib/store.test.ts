import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Game } from "../bindings/Game";
import { makeGame } from "../test-utils";
import * as api from "./api";
import { store } from "./store";

vi.mock("./api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));

function noop() {}

beforeEach(() => {
  vi.clearAllMocks();
  store.reset();
});

describe("store", () => {
  it("ignores out-of-order list responses", async () => {
    const holder: { resolve: (g: Game[]) => void } = { resolve: noop };
    vi.mocked(api.listGames)
      .mockImplementationOnce(() => new Promise((r) => (holder.resolve = r)))
      .mockResolvedValueOnce([makeGame({ id: "new" })]);
    const first = store.refresh();
    await store.refresh();
    holder.resolve([makeGame({ id: "stale" })]);
    await first;
    expect(store.games().map((g) => g.id)).toEqual(["new"]);
  });

  it("tracks running games and toasts errors", () => {
    vi.mocked(api.getGame).mockResolvedValue(makeGame());
    store.handleCoreEvent({ type: "gameStarted", id: "g1" });
    expect(store.isRunning("g1")).toBe(true);
    store.handleCoreEvent({ type: "gameStopped", id: "g1", sessionSeconds: 5 });
    expect(store.isRunning("g1")).toBe(false);
    store.handleCoreEvent({ type: "gameError", id: "g1", message: "boom" });
    expect(store.toasts()[0]?.message).toBe("boom");
  });

  it("keeps the selection by id across refreshes and clears it on removal", async () => {
    const a = makeGame({ id: "a" });
    const b = makeGame({ id: "b" });
    vi.mocked(api.listGames).mockResolvedValue([a, b]);
    await store.refresh();
    store.selectAndOpen("b");
    vi.mocked(api.listGames).mockResolvedValue([b, a]);
    await store.refresh();
    expect(store.selectedIndex()).toBe(0);
    store.handleCoreEvent({ type: "gameRemoved", id: "b" });
    expect(store.selectedId()).toBeNull();
    expect(store.detailsOpen()).toBe(false);
  });

  it("refetches when the query changes", async () => {
    vi.mocked(api.listGames).mockResolvedValue([]);
    store.setView("favorites");
    await vi.waitFor(() => expect(api.listGames).toHaveBeenCalled());
    expect(vi.mocked(api.listGames).mock.calls[0]?.[0]).toMatchObject({ favoritesOnly: true });
  });

  it("bumps cover versions on gamesChanged", () => {
    const game = makeGame({ coverPath: "x.jpg" });
    expect(store.coverUrl(game)).toBe("x.jpg?v=0");
    store.handleCoreEvent({ type: "gamesChanged", ids: ["g1"] });
    expect(store.coverUrl(game)).toBe("x.jpg?v=1");
  });

  it("shows queued notices on start and when the app announces new ones", async () => {
    const handlers: Record<string, () => void> = {};
    const { listen } = await import("@tauri-apps/api/event");
    vi.mocked(listen).mockImplementation((name, handler) => {
      handlers[name] = handler as () => void;
      return Promise.resolve(() => undefined);
    });
    vi.mocked(api.listProviders).mockResolvedValue([]);
    vi.mocked(api.runningGames).mockResolvedValue([]);
    vi.mocked(api.listGames).mockResolvedValue([]);
    vi.mocked(api.appInfo).mockResolvedValue({
      version: "1",
      dataDir: "d",
      logsDir: "l",
      portable: false,
      devTools: false,
    });
    vi.mocked(api.takeNotices)
      .mockResolvedValueOnce(["Questkeep was started from another build"])
      .mockResolvedValueOnce(["second"]);
    await store.init();
    await vi.waitFor(() =>
      expect(store.toasts().map((t) => t.message)).toContain(
        "Questkeep was started from another build",
      ),
    );
    handlers["app-notice"]?.();
    await vi.waitFor(() => expect(store.toasts().map((t) => t.message)).toContain("second"));
  });
});
