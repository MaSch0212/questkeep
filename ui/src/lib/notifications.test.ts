import { describe, expect, it } from "vitest";
import type { Notification } from "../bindings/Notification";
import {
  actionLabel,
  badgeCount,
  badgeHasSeenEntries,
  badgeKind,
  badgeText,
  levelLabel,
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

const updates = (overrides: Partial<Notification> = {}) =>
  entry({
    id: "u",
    level: "info",
    source: "plugins",
    action: { type: "pluginUpdates" },
    ...overrides,
  });

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
    expect(sourceLabel("plugins")).toBe("Plugins");
    expect(actionLabel({ type: "pluginUpdates" })).toBe("View updates");
  });

  it("describes the level of an entry for screen readers", () => {
    expect(levelLabel("error")).toBe("Error");
    expect(levelLabel("warning")).toBe("Warning");
    expect(levelLabel("info")).toBe("Information");
  });

  it("does not turn the badge red for information", () => {
    expect(badgeKind([entry({ level: "info" })])).toBe("info");
    expect(badgeKind([entry({ level: "info" }), entry()])).toBe("neutral");
  });

  describe("pending plugin updates", () => {
    it("counts an unread entry once", () => {
      expect(badgeCount([updates()])).toBe(1);
      expect(badgeCount([updates(), entry()])).toBe(2);
      expect(badgeHasSeenEntries([updates()])).toBe(false);
    });

    it("keeps counting a read entry", () => {
      expect(badgeCount([updates({ read: true })])).toBe(1);
      expect(badgeKind([updates({ read: true })])).toBe("info");
      expect(badgeHasSeenEntries([updates({ read: true })])).toBe(true);
      expect(badgeCount([updates({ read: true }), entry()])).toBe(2);
      expect(badgeKind([updates({ read: true }), entry()])).toBe("neutral");
    });

    it("is red when an unread error comes with it", () => {
      expect(badgeKind([updates({ read: true }), entry({ id: "e", level: "error" })])).toBe(
        "error",
      );
      expect(badgeCount([updates({ read: true }), entry({ id: "e", level: "error" })])).toBe(2);
    });

    it("does not count other read entries or an empty list", () => {
      expect(badgeCount([])).toBe(0);
      expect(badgeKind([])).toBeNull();
      expect(badgeCount([entry({ read: true }), entry({ read: true, level: "error" })])).toBe(0);
      expect(badgeHasSeenEntries([entry({ read: true })])).toBe(false);
    });

    it("cuts big counts off as before", () => {
      const many = Array.from({ length: 120 }, (_, i) => entry({ id: `x${i}` }));
      expect(badgeText(badgeCount([...many, updates({ read: true })]))).toBe("99+");
    });
  });
});
