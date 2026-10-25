import { fireEvent, render, screen } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { CoreEvent } from "../bindings/CoreEvent";
import * as api from "../lib/api";
import { store } from "../lib/store";
import { SyncIndicator } from "./SyncIndicator";

vi.mock("../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));

beforeEach(() => {
  vi.useFakeTimers();
  vi.clearAllMocks();
  store.reset();
  vi.mocked(api.listGames).mockResolvedValue([]);
  vi.mocked(api.scanAllLibraries).mockResolvedValue(undefined);
});

afterEach(() => {
  vi.useRealTimers();
});

const send = (event: CoreEvent) => store.handleCoreEvent(event);
const scanned = (provider: string, added: number, updated: number): CoreEvent => ({
  type: "libraryScanned",
  provider,
  summary: { added, updated, uninstalled: 0 },
});

describe("SyncIndicator", () => {
  it("is idle until a scan starts, then spins with the progress as its label", () => {
    render(() => <SyncIndicator />);
    const idle = screen.getByRole("button", { name: "Sync status" });
    expect(idle.querySelector(".icon--spin")).toBeNull();

    send({ type: "scansQueued", providers: ["steam", "epic", "gog"] });
    send({ type: "scanStarted", provider: "steam" });
    const busy = screen.getByRole("button", { name: "Syncing 1 of 3…" });
    expect(busy).toHaveAttribute("title", "Syncing 1 of 3…");
    expect(busy.querySelector(".icon--spin")).not.toBeNull();

    send(scanned("steam", 1, 0));
    send({ type: "scanStarted", provider: "epic" });
    expect(screen.getByRole("button", { name: "Syncing 2 of 3…" })).toBeInTheDocument();
  });

  it("shows one toast for the whole run, after the last store plus a short pause", () => {
    render(() => <SyncIndicator />);
    send({ type: "scansQueued", providers: ["steam", "epic", "gog"] });
    send({ type: "scanStarted", provider: "steam" });
    send(scanned("steam", 3, 1));
    send({ type: "scanStarted", provider: "epic" });
    send({ type: "scanFailed", provider: "epic", message: "No network" });
    send({ type: "scanStarted", provider: "gog" });
    send(scanned("gog", 0, 2));
    expect(store.toasts()).toHaveLength(0);
    vi.advanceTimersByTime(1499);
    expect(store.toasts()).toHaveLength(0);
    vi.advanceTimersByTime(1);
    expect(store.toasts().map((t) => t.message)).toEqual([
      "Sync finished: 3 added, 3 updated, 1 failed",
    ]);
    expect(screen.getByRole("button", { name: "Sync status" })).toBeInTheDocument();
  });

  it("reports a run without changes", () => {
    render(() => <SyncIndicator />);
    send({ type: "scanStarted", provider: "steam" });
    send(scanned("steam", 0, 0));
    vi.advanceTimersByTime(1500);
    expect(store.toasts().map((t) => t.message)).toEqual(["Sync finished: no changes"]);
  });

  it("lists every store of the run in the popover, with the finish time afterwards", async () => {
    vi.mocked(api.listProviders).mockResolvedValue([
      { id: "steam", name: "Steam", canScan: true, canScanOwned: true, canUninstall: true },
    ]);
    render(() => <SyncIndicator />);
    send({ type: "scansQueued", providers: ["steam", "epic", "gog"] });
    send({ type: "scanStarted", provider: "steam" });
    send(scanned("steam", 2, 5));
    send({ type: "scanStarted", provider: "epic" });
    await fireEvent.click(screen.getByRole("button", { name: "Syncing 2 of 3…" }));
    const dialog = screen.getByRole("dialog", { name: "Library sync" });
    const items = dialog.querySelectorAll("li");
    expect(items).toHaveLength(3);
    expect(items[0]).toHaveTextContent("Steam");
    expect(items[0]).toHaveTextContent("+2, ~5");
    expect(items[1]).toHaveTextContent("Epic Games");
    expect(items[1]).toHaveTextContent("Scanning…");
    expect(items[2]).toHaveTextContent("Waiting");
    expect(screen.getByRole("button", { name: "Sync all" })).toBeDisabled();
    expect(dialog).not.toHaveTextContent("Last sync finished");

    send(scanned("epic", 0, 0));
    send({ type: "scanStarted", provider: "gog" });
    send({ type: "scanFailed", provider: "gog", message: "Not signed in" });
    expect(dialog.querySelectorAll("li")[2]).toHaveTextContent("Not signed in");
    vi.advanceTimersByTime(1500);
    expect(dialog).toHaveTextContent("Last sync finished at");
    expect(screen.getByRole("button", { name: "Sync all" })).toBeEnabled();
  });

  it("starts a full sync with Sync all", async () => {
    render(() => <SyncIndicator />);
    await fireEvent.click(screen.getByRole("button", { name: "Sync status" }));
    expect(screen.getByText("No sync has run yet.")).toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: "Sync all" }));
    expect(api.scanAllLibraries).toHaveBeenCalledTimes(1);
  });

  it("closes on Escape and gives the focus back to the button", async () => {
    render(() => <SyncIndicator />);
    const button = screen.getByRole("button", { name: "Sync status" });
    await fireEvent.click(button);
    expect(button).toHaveAttribute("aria-expanded", "true");
    const syncAll = screen.getByRole("button", { name: "Sync all" });
    syncAll.focus();
    await fireEvent.keyDown(syncAll, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(button).toHaveAttribute("aria-expanded", "false");
    expect(document.activeElement).toBe(button);
  });

  it("closes when clicking elsewhere", async () => {
    render(() => <SyncIndicator />);
    await fireEvent.click(screen.getByRole("button", { name: "Sync status" }));
    await fireEvent.pointerDown(document.body);
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});
