import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import * as api from "./lib/api";
import { store } from "./lib/store";
import { makeGame } from "./test-utils";

vi.mock("./lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn<() => Promise<() => void>>().mockResolvedValue(() => {}),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn<() => void>() }));
vi.mock("@tauri-apps/plugin-opener", () => ({
  openPath: vi.fn<() => void>(),
  openUrl: vi.fn<() => void>(),
}));

type ResizeCallback = (entries: { contentRect: { width: number; height: number } }[]) => void;
const observers: ResizeCallback[] = [];
const originalResizeObserver = globalThis.ResizeObserver;

/** Pretends every observed element is now this big. */
function resizeAll(width: number, height = 700) {
  for (const callback of observers) callback([{ contentRect: { width, height } }]);
}

beforeEach(() => {
  observers.length = 0;
  globalThis.ResizeObserver = class {
    constructor(callback: ResizeCallback) {
      observers.push(callback);
    }
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
  vi.clearAllMocks();
  localStorage.clear();
  store.reset();
  vi.mocked(api.listProviders).mockResolvedValue([
    { id: "manual", name: "Manual", canScan: false, canScanOwned: false },
    { id: "steam", name: "Steam", canScan: true, canScanOwned: true },
  ]);
  vi.mocked(api.appInfo).mockResolvedValue({
    version: "0.1.0",
    dataDir: "d",
    logsDir: "l",
    portable: false,
    devTools: false,
  });
  vi.mocked(api.runningGames).mockResolvedValue([]);
});

afterEach(() => {
  globalThis.ResizeObserver = originalResizeObserver;
});

describe("App", () => {
  it("renders the games returned by list_games", async () => {
    vi.mocked(api.listGames).mockResolvedValue([
      makeGame({ id: "1", name: "Hades" }),
      makeGame({ id: "2", name: "Celeste" }),
    ]);
    render(() => <App />);
    expect(await screen.findByRole("gridcell", { name: "Hades" })).toBeInTheDocument();
    expect(screen.getByRole("gridcell", { name: "Celeste" })).toBeInTheDocument();
    expect(screen.getByText("2 games")).toBeInTheDocument();
    expect(await screen.findByRole("button", { name: "Steam" })).toBeInTheDocument();
  });

  it("shows an empty state", async () => {
    vi.mocked(api.listGames).mockResolvedValue([]);
    render(() => <App />);
    await waitFor(() => expect(screen.getByText(/No games to show/)).toBeInTheDocument());
  });

  it("switches to the list view and remembers it", async () => {
    vi.mocked(api.listGames).mockResolvedValue([makeGame({ id: "1", name: "Hades" })]);
    const { unmount } = render(() => <App />);
    const gridButton = await screen.findByRole("button", { name: "Grid view" });
    expect(gridButton).toHaveAttribute("aria-pressed", "true");
    await fireEvent.click(screen.getByRole("button", { name: "List view" }));
    expect(screen.getByRole("button", { name: "List view" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(await screen.findByRole("option", { name: "Hades" })).toBeInTheDocument();
    expect(screen.queryByRole("grid")).toBeNull();
    expect(localStorage.getItem("questkeep.viewMode")).toBe("list");

    // A fresh start picks the remembered view up again.
    unmount();
    store.reset();
    render(() => <App />);
    expect(await screen.findByRole("option", { name: "Hades" })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: "Grid view" }));
    expect(localStorage.getItem("questkeep.viewMode")).toBe("grid");
    expect(await screen.findByRole("gridcell", { name: "Hades" })).toBeInTheDocument();
  });

  it("opens settings as a page and returns to the library with the search intact", async () => {
    vi.mocked(api.listGames).mockResolvedValue([
      makeGame({ id: "1", name: "Hades" }),
      makeGame({ id: "2", name: "Celeste" }),
    ]);
    vi.mocked(api.getSettings).mockResolvedValue({
      scanOnStartup: true,
      downloadCovers: true,
      fetchMetadata: true,
      steamGridDbApiKey: null,
      providers: {},
    });
    render(() => <App />);
    await screen.findByRole("gridcell", { name: "Hades" });
    const search = screen.getByLabelText("Search games");
    await fireEvent.input(search, { target: { value: "hades" } });
    await waitFor(() => expect(store.query().search).toBe("hades"));

    await fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    expect(screen.getByRole("heading", { name: "Settings", level: 1 })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "About" })).toBeInTheDocument();
    expect(screen.queryByRole("grid")).toBeNull();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByRole("button", { name: "Settings" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    // The top bar stays mounted (keeping the search text) but is hidden.
    expect(screen.getByLabelText("Search games").closest("[hidden]")).not.toBeNull();

    await fireEvent.click(screen.getByRole("button", { name: "All games" }));
    expect(await screen.findByRole("grid")).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Settings", level: 1 })).toBeNull();
    expect(screen.getByLabelText("Search games").closest("[hidden]")).toBeNull();
    expect(screen.getByLabelText("Search games")).toHaveValue("hades");
    expect(store.query().search).toBe("hades");
  });

  it("clears the open state when the details panel is closed", async () => {
    vi.mocked(api.listGames).mockResolvedValue([makeGame({ id: "1", name: "Hades" })]);
    const { container } = render(() => <App />);
    await fireEvent.click(await screen.findByRole("gridcell", { name: "Hades" }));
    expect(container.querySelector(".tile--open")).not.toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "Close details" }));
    expect(container.querySelector(".tile--open")).toBeNull();
    expect(container.querySelector(".details")).toBeNull();

    // Same for the list view.
    await fireEvent.click(screen.getByRole("button", { name: "List view" }));
    await fireEvent.click(await screen.findByRole("option", { name: "Hades" }));
    expect(container.querySelector(".list-row--open")).not.toBeNull();
    await fireEvent.keyDown(window, { key: "Escape" });
    expect(container.querySelector(".list-row--open")).toBeNull();
  });

  it("shows details beside the library when wide and as a flyout when narrow", async () => {
    vi.mocked(api.listGames).mockResolvedValue([makeGame({ id: "1", name: "Hades" })]);
    const { container } = render(() => <App />);
    await fireEvent.click(await screen.findByRole("gridcell", { name: "Hades" }));
    const panel = () => container.querySelector(".details");
    expect(panel()).not.toHaveClass("details--split");

    resizeAll(1400);
    await waitFor(() => expect(panel()).toHaveClass("details--split"));
    resizeAll(900);
    await waitFor(() => expect(panel()).not.toHaveClass("details--split"));
    resizeAll(1100);
    await waitFor(() => expect(panel()).toHaveClass("details--split"));
  });
});
