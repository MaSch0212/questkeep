import { render, screen, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
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
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn<() => void>() }));

beforeEach(() => {
  vi.clearAllMocks();
  store.reset();
  vi.mocked(api.listProviders).mockResolvedValue([
    { id: "manual", name: "Manual", canScan: false },
    { id: "steam", name: "Steam", canScan: true },
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
});
