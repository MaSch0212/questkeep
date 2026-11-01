import { translator } from "../i18n";
import { gameSections } from "../i18n/gameSections";
import type { Achievement } from "../bindings/Achievement";
import type { Achievements } from "../bindings/Achievements";
import type { GameActivity } from "../bindings/GameActivity";
import type { GameQuery } from "../bindings/GameQuery";
import type { ListField } from "../bindings/ListField";
import type { TimeToBeat } from "../bindings/TimeToBeat";
import { valueKey } from "./filters";

const tr = translator(gameSections);

/**
 * How many links fit in one row of `available` pixels, keeping room for a "+N more" button
 * (`moreWidth`) whenever some are left out. `widths` are the links' own widths, `gap` the space
 * between two items.
 */
export function linksThatFit(
  widths: readonly number[],
  available: number,
  moreWidth: number,
  gap: number,
): number {
  const total = widths.reduce((sum, w, i) => sum + w + (i > 0 ? gap : 0), 0);
  if (total <= available) return widths.length;
  let used = moreWidth;
  let count = 0;
  for (const width of widths) {
    const next = used + gap + width;
    if (next > available) break;
    used = next;
    count += 1;
  }
  return count;
}

/** How many rows wrapped items of these widths take in `available` pixels. */
function wrappedRows(widths: readonly number[], available: number, gap: number): number {
  let rows = 0;
  let used = 0;
  for (const width of widths) {
    if (rows === 0 || used + gap + width > available) {
      rows += 1;
      used = width;
    } else {
      used += gap + width;
    }
  }
  return rows;
}

/**
 * How many chips fit in `rows` wrapped rows of `available` pixels, keeping room for a "+N" chip
 * (`moreWidth`) at the end whenever some are left out.
 */
export function chipsThatFit(
  widths: readonly number[],
  available: number,
  moreWidth: number,
  gap: number,
  rows: number,
): number {
  if (wrappedRows(widths, available, gap) <= rows) return widths.length;
  for (let count = widths.length - 1; count > 0; count--) {
    if (wrappedRows([...widths.slice(0, count), moreWidth], available, gap) <= rows) return count;
  }
  return 0;
}

export interface TtbSegment {
  key: "main" | "extra" | "complete";
  label: string;
  seconds: number;
  /** Percent of the bar. */
  left: number;
  width: number;
}

export interface TtbBar {
  segments: TtbSegment[];
  /** The user's playtime on the bar (percent), `null` without playtime. */
  you: number | null;
  /** Whether the "You" label sits left of its marker (near the end). */
  youAtEnd: boolean;
}

const TTB_LABELS = {
  main: "ttbMain",
  extra: "ttbExtra",
  complete: "ttbComplete",
} as const;

/** Where the user's marker counts as "near the end" (its label flips to the left). */
const NEAR_END = 80;

/**
 * The time-to-beat bar: each known length is a segment starting where the shorter one ends, all
 * scaled to the longest length (or the user's playtime, when longer). `null` without lengths.
 */
export function ttbBar(ttb: TimeToBeat | null, playtime: number): TtbBar | null {
  if (!ttb) return null;
  const lengths = (["main", "extra", "complete"] as const)
    .map((key) => ({ key, seconds: ttb[key] ?? 0 }))
    .filter((l) => l.seconds > 0);
  if (lengths.length === 0) return null;
  const scale = Math.max(playtime, ...lengths.map((l) => l.seconds));
  let previous = 0;
  const segments: TtbSegment[] = [];
  for (const { key, seconds } of lengths) {
    const start = Math.min(previous, seconds);
    segments.push({
      key,
      label: tr(TTB_LABELS[key]),
      seconds,
      left: (start / scale) * 100,
      width: ((seconds - start) / scale) * 100,
    });
    previous = Math.max(previous, seconds);
  }
  const you = playtime > 0 ? (playtime / scale) * 100 : null;
  return { segments, you, youAtEnd: you !== null && you > NEAR_END };
}

/** Whole hours, at least "<1h": time-to-beat lengths are rough anyway. */
export function formatHours(seconds: number): string {
  const hours = Math.round(seconds / 3600);
  return hours < 1 ? tr("lessThanHour") : tr("hoursShort", { hours });
}

/** The unlocked achievement the fewest players have, if any has a percentage. */
export function rarestUnlocked(achievements: Achievements): Achievement | null {
  let rarest: Achievement | null = null;
  for (const a of achievements.unlocked) {
    if (a.percent === null) continue;
    if (rarest === null || a.percent < (rarest.percent ?? Infinity)) rarest = a;
  }
  return rarest;
}

export interface ActivitySummary {
  total: number;
  thisWeek: number;
  /** Bar heights in percent of the busiest week, oldest first. */
  bars: number[];
}

/** Totals and bar heights; `null` when nothing was played in these weeks. */
export function activitySummary(activity: GameActivity): ActivitySummary | null {
  const total = activity.weeks.reduce((sum, w) => sum + w.seconds, 0);
  if (total === 0) return null;
  const busiest = Math.max(...activity.weeks.map((w) => w.seconds));
  return {
    total,
    thisWeek: activity.weeks.at(-1)?.seconds ?? 0,
    bars: activity.weeks.map((w) => (busiest > 0 ? (w.seconds / busiest) * 100 : 0)),
  };
}

/** Explore's preset for games with this value of a list field (a clicked chip or detail). */
export function exploreFor(field: ListField, value: string): Partial<GameQuery> {
  return {
    filter: {
      mode: "all",
      items: [{ kind: "list", field, mode: "any", values: [valueKey(value)] }],
    },
  };
}

export type LinkKind =
  | "store"
  | "wiki"
  | "news"
  | "community"
  | "achievements"
  | "workshop"
  | "support"
  | "web";

/** Which icon a link gets, from its name and address. */
export function linkKind(name: string, url: string): LinkKind {
  const text = `${name} ${url}`.toLowerCase();
  if (/store|shop/.test(name.toLowerCase())) return "store";
  if (/achievement/.test(text)) return "achievements";
  if (/workshop/.test(text)) return "workshop";
  if (/wiki/.test(text)) return "wiki";
  if (/news/.test(text)) return "news";
  if (/support|help/.test(text)) return "support";
  if (/community|discussion|forum|guide|reddit|discord/.test(text)) return "community";
  return "web";
}
