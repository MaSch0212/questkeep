import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { Show } from "solid-js";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../lib/api";
import { store } from "../lib/store";
import { TitleBar } from "./TitleBar";
import { UpdateDialog } from "./UpdateDialog";

vi.mock("../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));

const update: api.UpdateInfo = { version: "0.3.0", notes: "Faster grid.", date: null };
const handlers = new Map<string, (event: { payload: unknown }) => void>();

beforeEach(async () => {
  vi.clearAllMocks();
  store.reset();
  handlers.clear();
  vi.mocked(api.listProviders).mockResolvedValue([]);
  vi.mocked(api.listGames).mockResolvedValue([]);
  vi.mocked(api.listViews).mockResolvedValue([]);
  vi.mocked(api.countGames).mockResolvedValue([]);
  vi.mocked(api.appInfo).mockResolvedValue({
    version: "0.2.0",
    dataDir: "",
    logsDir: "",
    portable: false,
    devTools: false,
  });
  vi.mocked(api.runningGames).mockResolvedValue([]);
  vi.mocked(api.syncTimes).mockResolvedValue({ all: null, providers: {} });
  vi.mocked(api.listLaunchers).mockResolvedValue([]);
  vi.mocked(api.availableUpdate).mockResolvedValue(null);
  vi.mocked(api.takeUpdateRequest).mockResolvedValue(false);
  const { listen } = await import("@tauri-apps/api/event");
  vi.mocked(listen).mockImplementation((name, handler) => {
    handlers.set(name, handler as (event: { payload: unknown }) => void);
    return Promise.resolve(() => undefined);
  });
});

function renderShell() {
  render(() => (
    <>
      <TitleBar onAdd={() => undefined} />
      <Show when={store.updateOpen() && store.update()}>
        {(found) => <UpdateDialog update={found()} onClose={() => store.setUpdateOpen(false)} />}
      </Show>
    </>
  ));
}

describe("updates", () => {
  it("shows no update button while Questkeep is up to date", async () => {
    await store.init();
    renderShell();
    expect(screen.queryByRole("button", { name: /Update to/ })).toBeNull();
  });

  it("offers the update found after start in the title bar", async () => {
    vi.mocked(api.availableUpdate).mockResolvedValue(update);
    vi.mocked(api.installUpdate).mockRejectedValueOnce("Could not install the update: offline");
    await store.init();
    renderShell();
    await fireEvent.click(screen.getByRole("button", { name: "Update to 0.3.0" }));
    const dialog = screen.getByRole("dialog", { name: "Update available" });
    expect(dialog).toHaveTextContent("Questkeep 0.3.0 is available (you have 0.2.0)");
    expect(dialog).toHaveTextContent("Faster grid.");
    await fireEvent.click(screen.getByRole("button", { name: "Install and restart" }));
    expect(api.installUpdate).toHaveBeenCalled();
    expect(await screen.findByRole("alert")).toHaveTextContent("offline");
    await fireEvent.click(screen.getByRole("button", { name: "Later" }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("shows the button when a check finds an update later", async () => {
    await store.init();
    renderShell();
    handlers.get("update-available")?.({ payload: update });
    expect(await screen.findByRole("button", { name: "Update to 0.3.0" })).toBeTruthy();
  });

  it("opens the dialog when the tray asks for it", async () => {
    vi.mocked(api.availableUpdate).mockResolvedValue(update);
    vi.mocked(api.takeUpdateRequest).mockResolvedValueOnce(true);
    await store.init();
    renderShell();
    await waitFor(() =>
      expect(screen.getByRole("dialog", { name: "Update available" })).toBeTruthy(),
    );

    await fireEvent.click(screen.getByRole("button", { name: "Later" }));
    vi.mocked(api.takeUpdateRequest).mockResolvedValueOnce(true);
    handlers.get("show-update")?.({ payload: null });
    await waitFor(() =>
      expect(screen.getByRole("dialog", { name: "Update available" })).toBeTruthy(),
    );
  });
});
