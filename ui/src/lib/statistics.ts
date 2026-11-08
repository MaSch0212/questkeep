import type { ChartRange } from "../bindings/ChartRange";
import type { DaySeconds } from "../bindings/DaySeconds";
import type { DriveUsage } from "../bindings/DriveUsage";
import type { PlaytimeChart } from "../bindings/PlaytimeChart";
import { intlLocale } from "../i18n";

/** Hours for the statistics: "7,079 h", "11.4 h" (one decimal below 100), "0 h". */
export function hoursText(seconds: number): string {
  const hours = seconds / 3600;
  if (hours > 0 && hours < 0.05) return `<${(0.1).toLocaleString(intlLocale())} h`;
  const digits = hours > 0 && hours < 100 ? 1 : 0;
  return `${hours.toLocaleString(intlLocale(), {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  })} h`;
}

/** Memory as the round size it's sold as ("32 GB" for 31.8 GiB). */
export function memoryText(bytes: number): string {
  return `${Math.round(bytes / 1024 ** 3).toLocaleString(intlLocale())} GB`;
}

/** Shade 0–4 of a heatmap cell: 0 is nothing, then quarters of the busiest cell. */
export function heatLevel(value: number, max: number): number {
  if (value <= 0 || max <= 0) return 0;
  return Math.min(4, Math.max(1, Math.ceil((value / max) * 4)));
}

/** Calendar days (starting on a Monday) as week columns of seven. */
export function calendarWeeks(days: readonly DaySeconds[]): DaySeconds[][] {
  const weeks: DaySeconds[][] = [];
  for (let i = 0; i < days.length; i += 7) weeks.push(days.slice(i, i + 7));
  return weeks;
}

function localDate(label: string): Date {
  const [y, m, d] = label.split("-").map(Number);
  return new Date(y ?? 0, (m ?? 1) - 1, d ?? 1);
}

/** A chart label: weekday (7 days), day of month (30 days) or month name (12 months). */
export function chartLabel(label: string, range: ChartRange): string {
  const date = localDate(label);
  switch (range) {
    case "week":
      return date.toLocaleDateString(intlLocale(), { weekday: "short" });
    case "month":
      return String(date.getDate());
    case "year":
      return date.toLocaleDateString(intlLocale(), { month: "short" });
  }
}

/** The full date or month of a chart label, for tooltips. */
export function chartTitle(label: string, range: ChartRange): string {
  const date = localDate(label);
  return range === "year"
    ? date.toLocaleDateString(intlLocale(), { month: "long", year: "numeric" })
    : date.toLocaleDateString(intlLocale(), { weekday: "long", day: "numeric", month: "long" });
}

/** The sum of all series per bucket. */
export function chartTotals(chart: PlaytimeChart): number[] {
  return chart.labels.map((_, i) => chart.series.reduce((sum, s) => sum + (s.values[i] ?? 0), 0));
}

/** A rounded-up top for the chart's scale, in whole hours (at least one). */
export function chartScale(totals: readonly number[]): number {
  const hours = Math.max(1, ...totals.map((s) => s / 3600));
  const step = hours <= 5 ? 1 : hours <= 20 ? 5 : hours <= 100 ? 10 : 50;
  return Math.ceil(hours / step) * step * 3600;
}

/** A drive's games, other files and free space as shares of its size (0–1). */
export function driveShares(usage: DriveUsage): { games: number; other: number; free: number } {
  const total = usage.drive.totalBytes;
  if (total <= 0) return { games: 0, other: 0, free: 0 };
  const free = Math.min(total, usage.drive.freeBytes);
  const games = Math.min(total - free, usage.gamesBytes);
  return { games: games / total, other: (total - free - games) / total, free: free / total };
}

/** The busiest weekday (0 = Monday) and three-hour window of the hour grid, starting at a played
 *  hour ("Fri 20–23"), or null without data. */
export function busiestSlot(
  hours: readonly (readonly number[])[],
): { day: number; hour: number } | null {
  let best = { day: -1, hour: -1, seconds: 0 };
  hours.forEach((row, day) => {
    for (let hour = 0; hour < 24; hour++) {
      if ((row[hour] ?? 0) <= 0) continue;
      const seconds = (row[hour] ?? 0) + (row[(hour + 1) % 24] ?? 0) + (row[(hour + 2) % 24] ?? 0);
      if (seconds > best.seconds) best = { day, hour, seconds };
    }
  });
  return best.day < 0 ? null : { day: best.day, hour: best.hour };
}

/** A weekday name (0 = Monday) in the UI language. */
export function weekdayName(day: number, style: "short" | "long" = "short"): string {
  // 2024-01-01 was a Monday.
  return new Date(2024, 0, 1 + day).toLocaleDateString(intlLocale(), { weekday: style });
}
