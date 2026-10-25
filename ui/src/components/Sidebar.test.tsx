import { fireEvent, render, screen } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../lib/api";
import { store } from "../lib/store";
import { Sidebar } from "./Sidebar";

vi.mock("../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));

const STEAM_SVG =
  '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M0 0h24v24H0z"></path></svg>';

beforeEach(async () => {
  vi.clearAllMocks();
  store.reset();
  vi.mocked(api.listProviders).mockResolvedValue([
    {
      id: "manual",
      name: "Manual",
      icon: "<svg></svg>",
      canScan: false,
      canScanOwned: false,
      canUninstall: false,
      addGameOptions: [],
    },
    {
      id: "steam",
      name: "Steam",
      icon: STEAM_SVG,
      canScan: true,
      canScanOwned: true,
      canUninstall: true,
      addGameOptions: [],
    },
  ]);
  const { listen } = await import("@tauri-apps/api/event");
  vi.mocked(listen).mockResolvedValue(() => undefined);
  vi.mocked(api.appInfo).mockResolvedValue({
    version: "0",
    dataDir: "",
    logsDir: "",
    portable: false,
    devTools: false,
  });
  vi.mocked(api.runningGames).mockResolvedValue([]);
  vi.mocked(api.listGames).mockResolvedValue([]);
  vi.mocked(api.scanLibrary).mockResolvedValue({ added: 0, updated: 0, uninstalled: 0 });
  await store.init();
});

describe("Sidebar sources", () => {
  it("shows the sync status on the Sources heading", () => {
    const { container } = render(() => <Sidebar onAdd={vi.fn<() => void>()} />);
    const head = container.querySelector(".sidebar__section");
    expect(head).toHaveTextContent("Sources");
    expect(head?.querySelector('[aria-label="Sync status"]')).not.toBeNull();
  });

  it("shows each source with the provider's logo", () => {
    render(() => <Sidebar onAdd={vi.fn<() => void>()} />);
    const steam = screen.getByRole("button", { name: "Steam" });
    expect(steam.querySelector(".source-icon")?.innerHTML).toBe(STEAM_SVG);
  });

  it("offers a sync button only for stores that can scan", async () => {
    render(() => <Sidebar onAdd={vi.fn<() => void>()} />);
    expect(screen.queryByRole("button", { name: "Sync Manual" })).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "Sync Steam" }));
    expect(api.scanLibrary).toHaveBeenCalledWith("steam");
  });

  it("spins and disables a store's button while it is syncing", () => {
    render(() => <Sidebar onAdd={vi.fn<() => void>()} />);
    store.handleCoreEvent({ type: "scanStarted", provider: "steam" });
    const button = screen.getByRole("button", { name: "Syncing Steam…" });
    expect(button).toBeDisabled();
    expect(button.closest(".source-item")).toHaveClass("source-item--syncing");
  });
});

async function withPrism() {
  vi.mocked(api.listProviders).mockResolvedValue([
    {
      id: "manual",
      name: "Manual",
      icon: "<svg></svg>",
      canScan: false,
      canScanOwned: false,
      canUninstall: false,
      addGameOptions: [],
    },
    {
      id: "prism",
      name: "Prism Launcher",
      icon: "<svg></svg>",
      canScan: true,
      canScanOwned: false,
      canUninstall: false,
      addGameOptions: [{ id: "new-instance", label: "New Minecraft instance" }],
    },
  ]);
  store.reset();
  await store.init();
}

describe("Sidebar add game button", () => {
  it("is a plain button when no provider offers other ways", async () => {
    const onAdd = vi.fn<() => void>();
    render(() => <Sidebar onAdd={onAdd} />);
    expect(screen.queryByRole("button", { name: "More ways to add games" })).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "Add game" }));
    expect(onAdd).toHaveBeenCalled();
  });

  it("opens a menu with the providers' options and runs the chosen one", async () => {
    await withPrism();
    vi.mocked(api.addGameWithOption).mockResolvedValue(undefined);
    render(() => <Sidebar onAdd={vi.fn<() => void>()} />);
    const arrow = screen.getByRole("button", { name: "More ways to add games" });
    expect(arrow).toHaveAttribute("aria-expanded", "false");
    await fireEvent.click(arrow);
    expect(arrow).toHaveAttribute("aria-expanded", "true");
    const item = screen.getByRole("menuitem", { name: "New Minecraft instance" });
    await fireEvent.click(item);
    expect(api.addGameWithOption).toHaveBeenCalledWith("prism", "new-instance");
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("opens with the arrow keys and closes with Escape", async () => {
    await withPrism();
    render(() => <Sidebar onAdd={vi.fn<() => void>()} />);
    const arrow = screen.getByRole("button", { name: "More ways to add games" });
    await fireEvent.keyDown(arrow, { key: "ArrowDown" });
    const item = screen.getByRole("menuitem", { name: "New Minecraft instance" });
    await new Promise((resolve) => queueMicrotask(() => resolve(undefined)));
    expect(item).toHaveFocus();
    await fireEvent.keyDown(item, { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(arrow).toHaveFocus();
  });
});
