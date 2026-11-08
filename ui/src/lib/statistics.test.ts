import { describe, expect, it } from "vitest";
import type { DriveUsage } from "../bindings/DriveUsage";
import {
  busiestSlot,
  calendarWeeks,
  chartLabel,
  chartScale,
  chartTotals,
  driveShares,
  heatLevel,
  hoursText,
  memoryText,
} from "./statistics";

describe("statistics helpers", () => {
  it("formats hours with a decimal only for small values", () => {
    expect(hoursText(41_040)).toBe("11.4 h");
    expect(hoursText(25_484_400)).toBe("7,079 h");
    expect(hoursText(0)).toBe("0 h");
    expect(hoursText(60)).toBe("<0.1 h");
    expect(memoryText(34_141_347_840)).toBe("32 GB");
  });

  it("shades heatmap cells in quarters of the busiest one", () => {
    expect(heatLevel(0, 100)).toBe(0);
    expect(heatLevel(1, 100)).toBe(1);
    expect(heatLevel(50, 100)).toBe(2);
    expect(heatLevel(100, 100)).toBe(4);
    expect(heatLevel(5, 0)).toBe(0);
  });

  it("groups calendar days into weeks", () => {
    const days = Array.from({ length: 10 }, (_, i) => ({ day: `2026-11-${i + 1}`, seconds: i }));
    expect(calendarWeeks(days).map((w) => w.length)).toEqual([7, 3]);
  });

  it("labels chart buckets per range", () => {
    expect(chartLabel("2026-11-05", "month")).toBe("5");
    expect(chartLabel("2026-11-05", "week")).toBe("Thu");
    expect(chartLabel("2026-11", "year")).toBe("Nov");
  });

  it("sums series and rounds the scale up", () => {
    const totals = chartTotals({
      labels: ["a", "b"],
      series: [
        { key: "steam", name: null, values: [3600, 0] },
        { key: "epic", name: null, values: [7200, 1800] },
      ],
    });
    expect(totals).toEqual([10_800, 1800]);
    expect(chartScale(totals)).toBe(3 * 3600);
    expect(chartScale([0])).toBe(3600);
    expect(chartScale([23 * 3600])).toBe(30 * 3600);
  });

  it("splits a drive into games, other files and free space", () => {
    const usage = {
      drive: { totalBytes: 1000, freeBytes: 400 },
      gamesBytes: 450,
      games: 3,
    } as DriveUsage;
    expect(driveShares(usage)).toEqual({ games: 0.45, other: 0.15, free: 0.4 });
  });

  it("finds the busiest three hours", () => {
    const hours = Array.from({ length: 7 }, () => Array.from({ length: 24 }, () => 0));
    expect(busiestSlot(hours)).toBeNull();
    const friday = hours[4] ?? [];
    friday[20] = 3600;
    friday[21] = 3600;
    expect(busiestSlot(hours)).toEqual({ day: 4, hour: 20 });
  });
});
