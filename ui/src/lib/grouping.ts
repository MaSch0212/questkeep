import type { Category } from "../bindings/Category";
import type { CompletionStatus } from "../bindings/CompletionStatus";
import type { Game } from "../bindings/Game";
import type { GroupBy } from "../bindings/GroupBy";
import { storesOf } from "./copies";
import type { NavKey } from "./gridNav";
import { valueKey } from "./filters";
import { NO_CATEGORY } from "./views";

/** One group of a grouped view. A game with several categories is in each of their groups. */
export interface GameGroup {
  /**
   * Category id, store id, status id, value key or bucket, `NO_CATEGORY` for games without a value,
   * or "" for the single group of an ungrouped view.
   */
  key: string;
  label: string;
  /** The store, when grouped by store (for its icon). */
  source: string | null;
  games: Game[];
}

const collator = new Intl.Collator(undefined, { sensitivity: "base", numeric: true });

/** What grouping needs besides the games. */
export interface GroupContext {
  categories: readonly Category[];
  sourceName: (id: string) => string;
  /** Completion statuses in list order. */
  statuses: readonly CompletionStatus[];
  /** For the "last played" buckets; defaults to the current time. */
  now?: number;
}

interface Bucket {
  label: string;
  /** Whether a value (seconds, bytes, a score) belongs here. */
  test: (value: number) => boolean;
}

const HOUR = 3600;
const GB = 1024 ** 3;
const DAY = 86400;

const PLAYTIME_BUCKETS: Bucket[] = [
  { label: "Never played", test: (v) => v === 0 },
  { label: "Under 1 hour", test: (v) => v < HOUR },
  { label: "1–10 hours", test: (v) => v < 10 * HOUR },
  { label: "10–50 hours", test: (v) => v < 50 * HOUR },
  { label: "50–100 hours", test: (v) => v < 100 * HOUR },
  { label: "100 hours or more", test: () => true },
];

const SIZE_BUCKETS: Bucket[] = [
  { label: "Under 1 GB", test: (v) => v < GB },
  { label: "1–10 GB", test: (v) => v < 10 * GB },
  { label: "10–50 GB", test: (v) => v < 50 * GB },
  { label: "50–100 GB", test: (v) => v < 100 * GB },
  { label: "100 GB or more", test: () => true },
];

const SCORE_BUCKETS: Bucket[] = [
  { label: "90–100", test: (v) => v >= 90 },
  { label: "75–89", test: (v) => v >= 75 },
  { label: "50–74", test: (v) => v >= 50 },
  { label: "Under 50", test: () => true },
];

/** Seconds since last played. */
const LAST_PLAYED_BUCKETS: Bucket[] = [
  { label: "Today", test: (v) => v < DAY },
  { label: "This week", test: (v) => v < 7 * DAY },
  { label: "This month", test: (v) => v < 30 * DAY },
  { label: "This year", test: (v) => v < 365 * DAY },
  { label: "Over a year ago", test: () => true },
];

const LIST_FIELDS = {
  genre: "genres",
  tag: "tags",
  feature: "features",
  series: "series",
  platform: "platforms",
  ageRating: "ageRatings",
  developer: "developers",
  publisher: "publishers",
  gameMode: "gameModes",
  theme: "themes",
  perspective: "perspectives",
} as const satisfies Partial<Record<GroupBy, keyof Game>>;

/** The label of the group of games without a value. */
export function noValueLabel(by: GroupBy): string {
  switch (by) {
    case "category":
      return "No category";
    case "completionStatus":
      return "No status";
    case "lastPlayed":
      return "Never played";
    default:
      return "None";
  }
}

interface Placement {
  key: string;
  label: string;
  source: string | null;
  /** Position among the groups; equal ranks fall back to the label. */
  rank: number;
}

/** The bucket a value falls into; none without a value. */
function bucket(buckets: Bucket[], value: number | null | undefined): Placement[] {
  if (value === null || value === undefined) return [];
  const index = buckets.findIndex((b) => b.test(value));
  return [{ key: `b${index}`, label: buckets[index]?.label ?? "", source: null, rank: index }];
}

/** The groups a game belongs to; empty for "no value". */
function placements(game: Game, by: GroupBy, ctx: GroupContext): Placement[] {
  switch (by) {
    case "source":
      return storesOf(game).map((s) => ({ key: s, label: ctx.sourceName(s), source: s, rank: 0 }));
    case "category": {
      const order = new Map(ctx.categories.map((c, index) => [c.id, index]));
      return game.categories.flatMap((id) => {
        const rank = order.get(id);
        if (rank === undefined) return [];
        return [{ key: id, label: ctx.categories[rank]?.name ?? "", source: null, rank }];
      });
    }
    case "completionStatus": {
      const rank = ctx.statuses.findIndex((s) => s.id === game.completionStatus);
      const status = ctx.statuses[rank];
      if (!status) return [];
      return [{ key: status.id, label: status.name, source: null, rank }];
    }
    case "releaseYear": {
      const year = Number.parseInt(game.releaseDate?.slice(0, 4) ?? "", 10);
      if (Number.isNaN(year)) return [];
      // Newest first.
      return [{ key: String(year), label: String(year), source: null, rank: -year }];
    }
    case "playtime":
      return bucket(PLAYTIME_BUCKETS, game.playtimeSeconds);
    case "installSize":
      return bucket(SIZE_BUCKETS, game.installSize);
    case "criticScore":
      return bucket(SCORE_BUCKETS, game.criticScore?.value);
    case "lastPlayed": {
      const ms = game.lastPlayed === null ? Number.NaN : Date.parse(game.lastPlayed);
      if (Number.isNaN(ms)) return [];
      return bucket(LAST_PLAYED_BUCKETS, Math.max(0, ((ctx.now ?? Date.now()) - ms) / 1000));
    }
    case "none":
      return [];
    default: {
      const seen = new Set<string>();
      return game[LIST_FIELDS[by]].flatMap((name) => {
        const key = valueKey(name);
        if (seen.has(key)) return [];
        seen.add(key);
        return [{ key, label: name, source: null, rank: 0 }];
      });
    }
  }
}

/**
 * Splits the (already sorted) games into groups, keeping their order inside each group. A game
 * with several values (categories, stores, tags) is in each of their groups. Categories and
 * statuses follow their list order, buckets their natural order, years go newest first, other
 * values by name; the group of games without a value comes last. Without grouping there is one
 * group, key "".
 */
export function groupGames(games: readonly Game[], by: GroupBy, ctx: GroupContext): GameGroup[] {
  if (by === "none") return [{ key: "", label: "", source: null, games: [...games] }];
  const groups = new Map<string, GameGroup & { rank: number }>();
  for (const game of games) {
    const places = placements(game, by, ctx);
    if (places.length === 0) {
      places.push({ key: NO_CATEGORY, label: noValueLabel(by), source: null, rank: Infinity });
    }
    for (const place of places) {
      let group = groups.get(place.key);
      if (!group) {
        group = {
          key: place.key,
          label: place.label,
          source: place.source,
          rank: place.rank,
          games: [],
        };
        groups.set(place.key, group);
      }
      group.games.push(game);
    }
  }
  return [...groups.values()]
    .toSorted((a, b) => a.rank - b.rank || collator.compare(a.label, b.label))
    .map(({ rank: _rank, ...group }) => group);
}

/** One game's place in a grouped layout (the same game can have several). */
export interface Slot {
  game: Game;
  groupKey: string;
  /** Index of its row in `GroupedLayout.rows`. */
  row: number;
  column: number;
}

export type LayoutRow =
  | { kind: "header"; group: GameGroup; collapsed: boolean; top: number; height: number }
  | { kind: "games"; groupKey: string; first: number; count: number; top: number; height: number };

export type HeaderRow = Extract<LayoutRow, { kind: "header" }>;
export type GamesRow = Extract<LayoutRow, { kind: "games" }>;

export const asHeaderRow = (row: LayoutRow | undefined): HeaderRow | undefined =>
  row?.kind === "header" ? row : undefined;
export const asGamesRow = (row: LayoutRow | undefined): GamesRow | undefined =>
  row?.kind === "games" ? row : undefined;

/** Slot indices of a games row. */
export function rowSlots(row: GamesRow): number[] {
  return Array.from({ length: row.count }, (_, i) => row.first + i);
}

export interface GroupedLayout {
  rows: LayoutRow[];
  slots: Slot[];
  /** Full content height, padding included. */
  height: number;
}

export interface LayoutOptions {
  columns: number;
  rowHeight: number;
  headerHeight: number;
  /** Space above the first and below the last row. */
  padding: number;
  /** Whether groups get header rows (false for an ungrouped view). */
  headers: boolean;
  /** Keys of collapsed groups: only their header shows. */
  collapsed: ReadonlySet<string>;
}

/** Rows (headers and rows of up to `columns` games) with their offsets, and every game slot. */
export function layoutGroups(groups: readonly GameGroup[], options: LayoutOptions): GroupedLayout {
  const columns = Math.max(1, Math.floor(options.columns));
  const rows: LayoutRow[] = [];
  const slots: Slot[] = [];
  let top = options.padding;
  for (const group of groups) {
    const collapsed = options.headers && options.collapsed.has(group.key);
    if (options.headers) {
      rows.push({ kind: "header", group, collapsed, top, height: options.headerHeight });
      top += options.headerHeight;
    }
    if (collapsed) continue;
    for (let start = 0; start < group.games.length; start += columns) {
      const chunk = group.games.slice(start, start + columns);
      const row = rows.length;
      rows.push({
        kind: "games",
        groupKey: group.key,
        first: slots.length,
        count: chunk.length,
        top,
        height: options.rowHeight,
      });
      chunk.forEach((game, column) => slots.push({ game, groupKey: group.key, row, column }));
      top += options.rowHeight;
    }
  }
  return { rows, slots, height: rows.length > 0 ? top + options.padding : 0 };
}

/** Rows intersecting the viewport, extended by `overscan` rows on both sides. */
export function visibleRowRange(
  layout: GroupedLayout,
  scrollTop: number,
  viewportHeight: number,
  overscan: number,
): { start: number; end: number } {
  const { rows } = layout;
  if (rows.length === 0) return { start: 0, end: 0 };
  // First row whose bottom is below the viewport's top.
  let low = 0;
  let high = rows.length - 1;
  while (low < high) {
    const mid = (low + high) >> 1;
    const row = rows[mid];
    if (row && row.top + row.height <= scrollTop) low = mid + 1;
    else high = mid;
  }
  const bottom = scrollTop + Math.max(0, viewportHeight);
  let last = low;
  while (last + 1 < rows.length && (rows[last + 1]?.top ?? Infinity) < bottom) last++;
  return {
    start: Math.max(0, low - overscan),
    end: Math.min(rows.length, last + 1 + overscan),
  };
}

/**
 * Smallest scroll change that shows a row completely; scrolling up also shows its group header
 * when the header is right above it. Returns `scrollTop` when the row is already visible.
 */
export function scrollTopForRow(
  layout: GroupedLayout,
  index: number,
  scrollTop: number,
  viewportHeight: number,
  padding: number,
): number {
  const row = layout.rows[index];
  if (!row) return scrollTop;
  const above = layout.rows[index - 1];
  const top = above?.kind === "header" ? above.top : row.top;
  const bottom = row.top + row.height;
  if (top < scrollTop) return Math.max(0, top - (index <= 1 ? padding : 0));
  if (bottom > scrollTop + viewportHeight) return bottom - viewportHeight + padding;
  return scrollTop;
}

/** The next game row after (`step` 1) or before (-1) `row`, skipping headers. */
function gamesRow(layout: GroupedLayout, row: number, step: 1 | -1): number | null {
  for (let i = row + step; i >= 0 && i < layout.rows.length; i += step) {
    if (layout.rows[i]?.kind === "games") return i;
  }
  return null;
}

function slotIn(layout: GroupedLayout, row: number, column: number): number {
  const target = layout.rows[row];
  if (target?.kind !== "games") return -1;
  return target.first + Math.min(column, target.count - 1);
}

/**
 * The slot to select after `key`, moving through the rows as shown: left/right step through all
 * slots (across rows and groups), up/down go to the row above/below (skipping headers) in the
 * same column or the row's last game, PageUp/PageDown move `pageRows` rows, Home/End jump to the
 * first/last game. `-1` only when there are no slots; a `current` of -1 starts at the first.
 */
export function navigateSlots(
  layout: GroupedLayout,
  current: number,
  key: NavKey,
  pageRows: number,
): number {
  const count = layout.slots.length;
  if (count === 0) return -1;
  const last = count - 1;
  const slot = layout.slots[current];
  if (!slot) return key === "End" ? last : 0;
  switch (key) {
    case "ArrowLeft":
      return Math.max(0, current - 1);
    case "ArrowRight":
      return Math.min(last, current + 1);
    case "Home":
      return 0;
    case "End":
      return last;
    case "ArrowUp":
    case "ArrowDown": {
      const row = gamesRow(layout, slot.row, key === "ArrowUp" ? -1 : 1);
      return row === null ? current : slotIn(layout, row, slot.column);
    }
    case "PageUp":
    case "PageDown": {
      const step = key === "PageUp" ? -1 : 1;
      let row = slot.row;
      for (let i = 0; i < Math.max(1, Math.floor(pageRows)); i++) {
        const next = gamesRow(layout, row, step);
        if (next === null) break;
        row = next;
      }
      if (row === slot.row) return step < 0 ? 0 : last;
      return slotIn(layout, row, slot.column);
    }
  }
}

/** DOM id of a game's element; with groups the same game can show several times. */
export function slotKey(slot: Pick<Slot, "game" | "groupKey">): string {
  return slot.groupKey === "" ? slot.game.id : `${slot.game.id}:${slot.groupKey}`;
}

/**
 * The group header to pin at the top of a grouped view scrolled to `scrollTop`: the last header
 * at or above the top edge. `offset` (zero or negative) pushes it up while the next header
 * scrolls in under it. `null` while the first header is still below the top, or without groups.
 */
export function stickyHeader(
  layout: GroupedLayout,
  scrollTop: number,
): { row: HeaderRow; offset: number } | null {
  let current: HeaderRow | undefined;
  for (const row of layout.rows) {
    if (row.kind !== "header") continue;
    if (row.top > scrollTop) {
      if (!current) return null;
      return { row: current, offset: Math.min(0, row.top - scrollTop - current.height) };
    }
    current = row;
  }
  return current ? { row: current, offset: 0 } : null;
}
