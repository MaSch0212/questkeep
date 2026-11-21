import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Notification } from "../bindings/Notification";
import * as api from "../lib/api";
import { store } from "../lib/store";
import { NotificationBell } from "./NotificationBell";

vi.mock("../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));

function entry(overrides: Partial<Notification> = {}): Notification {
  return {
    id: "n1",
    time: "2026-11-15T20:14:00",
    level: "warning",
    source: "launchers",
    title: "Launcher",
    message: "Ubisoft Connect did not close.",
    gameId: null,
    action: null,
    read: false,
    ...overrides,
  };
}

async function load(list: Notification[]) {
  vi.mocked(api.notifications).mockResolvedValue(list);
  await store.loadNotifications();
}

const badge = () => document.querySelector(".titlebar__badge");

beforeEach(() => {
  vi.clearAllMocks();
  store.reset();
  vi.mocked(api.notifications).mockResolvedValue([]);
});

describe("NotificationBell", () => {
  it("shows the unread count, in the error style when one is an error", async () => {
    await load([entry(), entry({ id: "n2", level: "error", message: "Boom" })]);
    render(() => <NotificationBell />);
    expect(screen.getByRole("button", { name: "Notifications, 2 unread" })).toBeInTheDocument();
    expect(badge()).toHaveTextContent("2");
    expect(badge()).toHaveClass("titlebar__badge--error");
  });

  it("shows a neutral badge for warnings only and none without unread entries", async () => {
    await load([entry()]);
    render(() => <NotificationBell />);
    expect(screen.getByRole("button", { name: "Notifications, 1 unread" })).toBeInTheDocument();
    expect(badge()).not.toHaveClass("titlebar__badge--error");
    await load([entry({ read: true })]);
    expect(screen.getByRole("button", { name: "Notifications" })).toBeInTheDocument();
    expect(badge()).toBeNull();
  });

  it("lists the entries and marks them read when opened", async () => {
    await load([
      entry({ id: "n1", message: "First" }),
      entry({ id: "n2", level: "error", source: "scripts", message: "Second", read: true }),
    ]);
    render(() => <NotificationBell />);
    await fireEvent.click(screen.getByRole("button", { name: "Notifications, 1 unread" }));
    const popover = screen.getByRole("dialog", { name: "Notifications" });
    const items = within(popover).getAllByRole("listitem");
    expect(items).toHaveLength(2);
    expect(items[0]).toHaveTextContent("First");
    expect(items[0]).toHaveTextContent("Launchers");
    expect(items[0]).toHaveClass("notif--unread");
    expect(items[1]).toHaveTextContent("Scripts");
    expect(items[1]).not.toHaveClass("notif--unread");
    expect(api.markNotificationsRead).toHaveBeenCalled();
    await waitFor(() => expect(badge()).toBeNull());
  });

  it("says so when there is nothing", async () => {
    render(() => <NotificationBell />);
    await fireEvent.click(screen.getByRole("button", { name: "Notifications" }));
    expect(screen.getByText(/No notifications/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Clear all" })).toBeDisabled();
  });

  it("runs an entry's action and closes", async () => {
    await load([
      entry({ id: "a", message: "Log", action: { type: "openFile", path: "C:\\x\\run.log" } }),
      entry({ id: "b", message: "Store", action: { type: "storeSettings", store: "epic" } }),
    ]);
    render(() => <NotificationBell />);
    await fireEvent.click(screen.getByRole("button", { name: "Notifications, 2 unread" }));
    await fireEvent.click(screen.getByRole("button", { name: "Open log" }));
    expect(api.openPath).toHaveBeenCalledWith("C:\\x\\run.log");
    expect(screen.queryByRole("dialog", { name: "Notifications" })).toBeNull();

    await fireEvent.click(screen.getByRole("button", { name: "Notifications" }));
    await fireEvent.click(screen.getByRole("button", { name: "Store settings" }));
    expect(store.page()).toBe("settings");
    expect(store.takeSettingsSection()).toBe("stores");
  });

  it("shows a plugin update entry as information and opens the Plugin updates dialog", async () => {
    await load([
      entry({
        id: "u",
        level: "info",
        source: "plugins",
        message: "1 plugin update available: BSManager 0.1.0 → 0.2.0",
        action: { type: "pluginUpdates" },
      }),
    ]);
    render(() => <NotificationBell />);
    expect(badge()).not.toHaveClass("titlebar__badge--error");
    await fireEvent.click(screen.getByRole("button", { name: "Notifications, 1 unread" }));
    const item = within(screen.getByRole("dialog", { name: "Notifications" })).getByRole(
      "listitem",
    );
    expect(item).toHaveTextContent("BSManager 0.1.0 → 0.2.0");
    expect(item).toHaveTextContent("Plugins");
    expect(within(item).getByRole("img", { name: "Information" })).toHaveClass("notif__dot--info");
    expect(store.pluginUpdatesOpen()).toBe(false);
    await fireEvent.click(within(item).getByRole("button", { name: "View updates" }));
    expect(store.pluginUpdatesOpen()).toBe(true);
    expect(screen.queryByRole("dialog", { name: "Notifications" })).toBeNull();
  });

  it("keeps the badge for pending plugin updates after opening, until dismissed", async () => {
    await load([
      entry({ id: "w", message: "Warn" }),
      entry({
        id: "u",
        level: "info",
        source: "plugins",
        message: "1 plugin update available",
        action: { type: "pluginUpdates" },
      }),
    ]);
    render(() => <NotificationBell />);
    expect(badge()).toHaveTextContent("2");
    await fireEvent.click(screen.getByRole("button", { name: "Notifications, 2 unread" }));
    expect(api.markNotificationsRead).toHaveBeenCalled();
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Notifications, 1 needs attention" }),
      ).toBeInTheDocument(),
    );
    expect(badge()).toHaveTextContent("1");
    expect(badge()).not.toHaveClass("titlebar__badge--error");
    await fireEvent.click(
      screen.getByRole("button", { name: "Dismiss notification: 1 plugin update available" }),
    );
    await waitFor(() => expect(badge()).toBeNull());
  });

  it("dismisses one entry and clears all", async () => {
    await load([entry({ id: "a", message: "One" }), entry({ id: "b", message: "Two" })]);
    render(() => <NotificationBell />);
    await fireEvent.click(screen.getByRole("button", { name: "Notifications, 2 unread" }));
    await fireEvent.click(screen.getByRole("button", { name: "Dismiss notification: One" }));
    expect(api.dismissNotification).toHaveBeenCalledWith("a");
    expect(screen.queryByText("One")).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "Clear all" }));
    expect(api.clearNotifications).toHaveBeenCalled();
    expect(screen.getByText(/No notifications/)).toBeInTheDocument();
  });

  it("opens on request (shortcut, command bar, system notification) and closes on Escape", async () => {
    render(() => <NotificationBell />);
    expect(screen.queryByRole("dialog", { name: "Notifications" })).toBeNull();
    store.showNotifications();
    const popover = await screen.findByRole("dialog", { name: "Notifications" });
    await fireEvent.keyDown(popover, { key: "Escape" });
    expect(screen.queryByRole("dialog", { name: "Notifications" })).toBeNull();
  });
});

describe("error toasts and the bell", () => {
  it("records the app's own error toasts but not information or the core's errors", async () => {
    store.pushToast("info", "Synced 12 games");
    expect(api.addNotification).not.toHaveBeenCalled();
    store.pushToast("error", "Could not save");
    await waitFor(() =>
      expect(api.addNotification).toHaveBeenCalledWith(
        expect.objectContaining({
          level: "error",
          source: "app",
          message: "Could not save",
          gameId: null,
        }),
      ),
    );
    vi.mocked(api.addNotification).mockClear();
    store.handleCoreEvent({ type: "gameError", id: "g1", message: "Failed to start" });
    expect(store.toasts().at(-1)?.message).toBe("Failed to start");
    expect(api.addNotification).not.toHaveBeenCalled();
  });

  it("reloads the list on core events", async () => {
    vi.mocked(api.notifications).mockResolvedValue([entry({ id: "z" })]);
    store.handleCoreEvent({ type: "notificationsChanged" });
    await waitFor(() => expect(store.notifications().map((n) => n.id)).toEqual(["z"]));
  });
});
