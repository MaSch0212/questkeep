import { describe, expect, it } from "vitest";
import { dayLabel, hoursLabel, relativeDay, startupTarget } from "./startPage";

describe("startupTarget", () => {
  const views = ["all", "fav"];

  it("opens the start page, or the first view while it is off", () => {
    expect(startupTarget({ kind: "start" }, true, null, views)).toEqual({ kind: "home" });
    expect(startupTarget({ kind: "start" }, false, null, views)).toEqual({
      kind: "view",
      id: null,
    });
  });

  it("opens a chosen view while it exists", () => {
    expect(startupTarget({ kind: "view", id: "fav" }, true, null, views)).toEqual({
      kind: "view",
      id: "fav",
    });
    expect(startupTarget({ kind: "view", id: "gone" }, true, null, views)).toEqual({
      kind: "view",
      id: null,
    });
  });

  it("opens the page that was open last", () => {
    const last = { kind: "last" } as const;
    expect(startupTarget(last, true, "fav", views)).toEqual({ kind: "view", id: "fav" });
    expect(startupTarget(last, true, "home", views)).toEqual({ kind: "home" });
    expect(startupTarget(last, false, "home", views)).toEqual({ kind: "view", id: null });
    expect(startupTarget(last, true, null, views)).toEqual({ kind: "home" });
    expect(startupTarget(last, true, "gone", views)).toEqual({ kind: "view", id: null });
  });
});

describe("relativeDay", () => {
  const now = new Date(2026, 9, 31, 14, 0);

  it("counts days and weeks back in local time", () => {
    expect(relativeDay(new Date(2026, 9, 31, 0, 5).toISOString(), now)).toBe("Today");
    expect(relativeDay(new Date(2026, 9, 30, 23, 59).toISOString(), now)).toBe("Yesterday");
    expect(relativeDay(new Date(2026, 9, 28, 12).toISOString(), now)).toBe("3 days ago");
    expect(relativeDay(new Date(2026, 9, 24, 12).toISOString(), now)).toBe("1 week ago");
    expect(relativeDay(new Date(2026, 9, 10, 12).toISOString(), now)).toBe("3 weeks ago");
  });

  it("shows a short date after four weeks, with the year when it differs", () => {
    expect(relativeDay(new Date(2026, 8, 18, 12).toISOString(), now)).toBe("18 Sep");
    expect(relativeDay(new Date(2024, 8, 18, 12).toISOString(), now)).toBe("18 Sep 2024");
  });
});

describe("labels", () => {
  it("formats hours with one decimal and a dash for none", () => {
    expect(hoursLabel(5400)).toBe("1.5");
    expect(hoursLabel(0)).toBe("–");
    expect(hoursLabel(60)).toBe("<0.1");
  });

  it("names today and other days by weekday", () => {
    const now = new Date(2026, 9, 31, 14, 0); // a Saturday
    expect(dayLabel("2026-10-31", now)).toBe("Today");
    expect(dayLabel("2026-10-30", now)).toBe("Fri");
  });
});
