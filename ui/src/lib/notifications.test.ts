import { describe, expect, it } from "vitest";
import type { Notification } from "../bindings/Notification";
import {
  actionLabel,
  badgeKind,
  badgeText,
  notificationTime,
  sourceLabel,
  unreadCount,
} from "./notifications";

function entry(overrides: Partial<Notification> = {}): Notification {
  return {
    id: "n1",
    time: "2026-11-15T20:14:00",
    level: "warning",
    source: "app",
    title: "Title",
    message: "Message",
    gameId: null,
    action: null,
    read: false,
    ...overrides,
  };
}

describe("notifications", () => {
  it("counts only unread entries", () => {
    expect(unreadCount([])).toBe(0);
    expect(unreadCount([entry(), entry({ read: true }), entry()])).toBe(2);
  });

  it("shows no badge without unread entries", () => {
    expect(badgeKind([])).toBeNull();
    expect(badgeKind([entry({ read: true, level: "error" })])).toBeNull();
  });

  it("colors the badge as an error only when an unread entry is one", () => {
    expect(badgeKind([entry(), entry({ read: true, level: "error" })])).toBe("neutral");
    expect(badgeKind([entry(), entry({ level: "error" })])).toBe("error");
  });

  it("cuts large counts off", () => {
    expect(badgeText(3)).toBe("3");
    expect(badgeText(100)).toBe("99+");
  });

  it("writes today's and yesterday's entries with the day and the time", () => {
    const now = new Date("2026-11-15T21:00:00");
    expect(notificationTime("2026-11-15T20:14:00", now)).toMatch(/^Today, .*14/);
    expect(notificationTime("2026-11-14T22:41:00", now)).toMatch(/^Yesterday, .*41/);
    expect(notificationTime("garbage", now)).toBe("");
  });

  it("names sources and actions", () => {
    expect(sourceLabel("scripts")).toBe("Scripts");
    expect(actionLabel({ type: "openFile", path: "a.log" })).toBe("Open log");
    expect(actionLabel({ type: "openFile", path: "a.txt" })).toBe("Open file");
    expect(actionLabel({ type: "storeSettings", store: "epic" })).toBe("Store settings");
    expect(actionLabel({ type: "editScript", id: "s" })).toBe("Edit script");
  });
});
