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
    },
    {
      id: "steam",
      name: "Steam",
      icon: STEAM_SVG,
      canScan: true,
      canScanOwned: true,
      canUninstall: true,
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
