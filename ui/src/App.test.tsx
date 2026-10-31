import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import * as api from "./lib/api";
import { defaultQuery } from "./lib/query";
import { store } from "./lib/store";
import type { ViewLayout } from "./bindings/ViewLayout";
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
    {
      id: "manual",
      name: "Manual",
      icon: "<svg></svg>",
      canScan: false,
      canScanOwned: false,
      canUninstall: false,
      installed: true,
      syncable: false,
      addGameOptions: [],
    },
    {
      id: "steam",
      name: "Steam",
      icon: "<svg></svg>",
      canScan: true,
      canScanOwned: true,
      canUninstall: true,
      installed: true,
      syncable: true,
      addGameOptions: [],
    },
  ]);
  vi.mocked(api.appInfo).mockResolvedValue({
    version: "0.1.0",
    dataDir: "d",
    logsDir: "l",
    portable: false,
    devTools: false,
  });
  vi.mocked(api.runningGames).mockResolvedValue([]);
  vi.mocked(api.countGames).mockResolvedValue([]);
  vi.mocked(api.syncTimes).mockResolvedValue({ all: null, providers: {} });
  vi.mocked(api.listLaunchers).mockResolvedValue([]);
  vi.mocked(api.updateView).mockResolvedValue(undefined as never);
  mockViews();
});

function mockViews(layout: ViewLayout = "grid") {
  vi.mocked(api.listViews).mockResolvedValue([
    {
      id: "library",
      name: "Library",
      views: [
        {
          id: "all",
          name: "All games",
          icon: "library",
          query: defaultQuery(),
          layout,
          group: "none",
        },
      ],
    },
  ]);
}

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
    expect(await screen.findByRole("button", { name: /All games/ })).toHaveAttribute(
      "aria-current",
      "page",
    );
  });

  it("shows an empty state", async () => {
    vi.mocked(api.listGames).mockResolvedValue([]);
    render(() => <App />);
    await waitFor(() => expect(screen.getByText(/No games to show/)).toBeInTheDocument());
  });

  it("switches the layout as an unsaved change of the view", async () => {
    vi.mocked(api.listGames).mockResolvedValue([makeGame({ id: "1", name: "Hades" })]);
    render(() => <App />);
    const gridButton = await screen.findByRole("button", { name: "Grid view" });
    expect(gridButton).toHaveAttribute("aria-pressed", "true");
    await fireEvent.click(screen.getByRole("button", { name: "List view" }));
    expect(screen.getByRole("button", { name: "List view" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(await screen.findByRole("option", { name: "Hades" })).toBeInTheDocument();
    expect(screen.queryByRole("grid")).toBeNull();
    expect(store.hasDraft("all")).toBe(true);
    expect(screen.getByRole("group", { name: "Unsaved changes to All games" })).toBeVisible();
    expect(api.updateView).not.toHaveBeenCalled();
  });

  it("moves the layout remembered before views existed into All games", async () => {
    localStorage.setItem("questkeep.viewMode", "list");
    vi.mocked(api.listGames).mockResolvedValue([makeGame({ id: "1", name: "Hades" })]);
    vi.mocked(api.updateView).mockImplementation(async () => {
      mockViews("list");
      return undefined as never;
    });
    render(() => <App />);
    expect(await screen.findByRole("option", { name: "Hades" })).toBeInTheDocument();
    expect(api.updateView).toHaveBeenCalledWith("all", { layout: "list" });
    expect(localStorage.getItem("questkeep.viewMode")).toBeNull();
    expect(store.hasDraft("all")).toBe(false);
  });

  it("switches to the details view: narrow list plus page, no side panel", async () => {
    vi.mocked(api.listGames).mockResolvedValue([
      makeGame({ id: "1", name: "Hades" }),
      makeGame({ id: "2", name: "Celeste" }),
    ]);
    render(() => <App />);
    await fireEvent.click(await screen.findByRole("button", { name: "Details view" }));
    expect(screen.getByRole("button", { name: "Details view" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(await screen.findByRole("option", { name: "Hades" })).toBeInTheDocument();
    // First game selected; exactly one details region (the page), without a close button.
    await waitFor(() =>
      expect(screen.getByRole("complementary", { name: "Game details" })).toHaveTextContent(
        "Hades",
      ),
    );
    expect(screen.getAllByRole("complementary", { name: "Game details" })).toHaveLength(1);
    expect(screen.queryByRole("button", { name: "Close details" })).toBeNull();
    expect(screen.queryByRole("grid")).toBeNull();
    // Escape does not close the page.
    await fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.getByRole("complementary", { name: "Game details" })).toBeInTheDocument();
  });

  it("opens settings as a page and returns to the library with the filter intact", async () => {
    vi.mocked(api.listGames).mockResolvedValue([
      makeGame({ id: "1", name: "Hades" }),
      makeGame({ id: "2", name: "Celeste" }),
    ]);
    vi.mocked(api.getSettings).mockResolvedValue({
      scanOnStartup: true,
      downloadCovers: true,
      fetchMetadata: true,
      steamGridDbApiKey: null,
      igdbClientId: null,
      igdbClientSecret: null,
      closeToTray: true,
      minimizeToTray: false,
      startWithSystem: false,
      startMinimized: true,
      detectExternalLaunches: true,
      providers: {},
    });
    render(() => <App />);
    await screen.findByRole("gridcell", { name: "Hades" });
    await fireEvent.click(
      screen.getByRole("button", { name: "Filter this view by name (Ctrl+F)" }),
    );
    const search = screen.getByLabelText("Filter this view by name");
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
    expect(screen.getByLabelText("Filter this view by name").closest("[hidden]")).not.toBeNull();

    await fireEvent.click(screen.getByRole("button", { name: /All games/ }));
    expect(await screen.findByRole("grid")).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Settings", level: 1 })).toBeNull();
    expect(screen.getByLabelText("Filter this view by name").closest("[hidden]")).toBeNull();
    expect(screen.getByLabelText("Filter this view by name")).toHaveValue("hades");
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
